//! Gröbner bases with explicit limits, for solving tier 2.
//!
//! Tier 2 of the solver handles polynomial systems such as `e·e = e` for an unknown element
//! `e = Σ c_k b_k` of a finite-dimensional algebra: the coefficients of `e·e − e` are quadratic
//! equations in the unknowns `c_k`. Over a field `F` (ℚ or ℤ/p) this module computes the reduced
//! Gröbner basis of the ideal `I` they generate and classifies the zero set `V(I)`:
//!
//! - **none**: the reduced basis is `{1}`, so `I` is the unit ideal and `V(I)` is empty, even over
//!   the algebraic closure (Hilbert's Nullstellensatz);
//! - **finite**: `I` is zero-dimensional. The number of solutions over the algebraic closure
//!   counted with multiplicity is `dim_F F[x]/I`, the number of standard monomials (monomials
//!   divisible by no leading monomial). The number of distinct solutions is the same count for
//!   the radical `√I`, computed by Seidenberg's lemma: `√I = I + (r_1, …, r_n)` where `r_i` is the
//!   square-free part of the minimal polynomial of `x_i` on `F[x]/I`. The solutions with
//!   coordinates in `F` come from a lexicographic basis (converted from the grevlex one by FGLM)
//!   by back-substitution, finding the roots in `F` of one univariate polynomial at a time;
//! - **family**: `I` has positive Krull dimension `d`, the largest size of a set `S` of variables
//!   such that no leading monomial uses only variables of `S` (the leading-term ideal has the
//!   same Hilbert polynomial as `I`). The reduced basis describes the family;
//! - **limit reached**: a limit of [`Limits`] was hit; the statistics say how far it got.
//!
//! Every phase counts its work in deterministic *steps* (one S-pair, one elementary reduction
//! `p ← p − c·m·g`, one row operation, one trial division, …) against `Limits::max_steps`, so the
//! solver never runs unbounded and behaves identically on every platform (no wall-clock time,
//! which is unavailable on `wasm32-unknown-unknown`).
//!
//! The main computation is Buchberger's algorithm in the degree reverse lexicographic order
//! (grevlex), with the Gebauer–Möller installation of Buchberger's product and chain criteria and
//! the sugar selection strategy. Polynomials live in an internal sparse representation with their
//! own term order; [`PolyRing`]'s graded lexicographic order is used only for input and output.
//! Variables are ordered `x_1 > x_2 > ⋯` as in the ring, so in the lexicographic order the last
//! variable is the one eliminated down to a univariate polynomial.
//!
//! Roots of univariate polynomials are found in the prime subfield: over ℤ/p by `gcd(f, x^p − x)`
//! and equal-degree splitting (brute force for small `p`); in characteristic 0 by the rational
//! root theorem on the primitive integer multiple, with the divisor enumeration counted against
//! the step limit. Coefficients are recognized as rational numbers by
//! their canonical text form and checked by mapping them back into the field.

use crate::error::{Error, Result};
use crate::poly::{Exps, Monomial, Poly, PolyRing};
use crate::ring::Ring;
use num_bigint::BigInt;
use num_integer::Integer;
use num_traits::{One, Signed, Zero};
use serde::Serialize;
use smallvec::{SmallVec, smallvec};
use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::fmt;

// ---------------------------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------------------------

/// Explicit bounds on a Gröbner basis computation and on solving.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Limits {
    /// Largest number of polynomials in the (minimal) basis during Buchberger's algorithm.
    pub max_basis: usize,
    /// Largest total degree of an S-pair or basis element in the main (grevlex) computation.
    pub max_degree: i64,
    /// Total work budget in steps, shared by every phase.
    pub max_steps: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_basis: 2_000,
            max_degree: 64,
            max_steps: 5_000_000,
        }
    }
}

/// Which limit stopped a computation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LimitKind {
    BasisSize,
    Degree,
    Steps,
}

impl fmt::Display for LimitKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            LimitKind::BasisSize => "basis size",
            LimitKind::Degree => "degree",
            LimitKind::Steps => "step",
        })
    }
}

/// Work statistics, reported with every answer.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Stats {
    /// Largest basis reached in any Buchberger run.
    pub basis_size: usize,
    /// Largest total degree of a basis element or S-pair.
    pub max_degree: i64,
    /// Work steps used, in all phases.
    pub steps: u64,
    /// S-pairs reduced.
    pub pairs: u64,
    /// S-pairs that reduced to zero (useless pairs the criteria did not catch).
    pub zero_reductions: u64,
}

/// A monomial order for [`groebner_basis`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TermOrder {
    /// Degree reverse lexicographic order: by total degree, then the monomial with the smaller
    /// exponent of the last variable where they differ is larger.
    Grevlex,
    /// Lexicographic order with `x_1 > x_2 > ⋯`.
    Lex,
}

/// A reduced Gröbner basis, as polynomials of the ring (printed in its graded lex order).
#[derive(Clone, Debug, PartialEq)]
pub struct Basis<E> {
    /// The basis, sorted by increasing leading monomial in `order`; `[1]` for the unit ideal.
    pub polys: Vec<Poly<E>>,
    pub order: TermOrder,
    pub stats: Stats,
}

/// The answer of [`solve`].
#[derive(Clone, Debug, PartialEq)]
pub enum Solution<E> {
    /// No solutions over the algebraic closure: the ideal is the unit ideal.
    None,
    /// Finitely many solutions over the algebraic closure.
    Finite(FiniteSolutions<E>),
    /// A positive-dimensional zero set.
    Family(Family<E>),
    /// A limit was reached.
    LimitReached {
        limit: LimitKind,
        /// The phase that stopped.
        phase: String,
        /// What was established before the limit, if anything.
        known: Option<String>,
    },
}

/// A zero-dimensional system.
#[derive(Clone, Debug, PartialEq)]
pub struct FiniteSolutions<E> {
    /// Solutions over the algebraic closure counted with multiplicity: `dim F[x]/I`.
    pub with_multiplicity: usize,
    /// Distinct solutions over the algebraic closure: `dim F[x]/√I`.
    pub distinct: usize,
    /// The solutions with every coordinate in the prime subfield of the field (all of ℚ or
    /// ℤ/p), one value per variable in ring order.
    pub points: Vec<Vec<E>>,
    /// True if `points` lists every solution over the algebraic closure.
    pub all_found: bool,
    /// Reduced grevlex Gröbner basis.
    pub basis: Vec<Poly<E>>,
    /// Reduced lexicographic Gröbner basis: a triangular description of all solutions.
    pub lex_basis: Vec<Poly<E>>,
    /// Why the search for points in the field was incomplete, if it was.
    pub note: Option<String>,
}

/// A positive-dimensional system.
#[derive(Clone, Debug, PartialEq)]
pub struct Family<E> {
    /// The Krull dimension of the ideal (the dimension of its largest component).
    pub dimension: usize,
    /// A largest set of variables independent modulo the ideal (indices into the ring's
    /// variables): the largest component is locally parametrized by them.
    pub independent: Vec<usize>,
    /// Reduced grevlex Gröbner basis: the generators describing the family.
    pub basis: Vec<Poly<E>>,
}

/// An answer with the work it took.
#[derive(Clone, Debug, PartialEq)]
pub struct Solved<E> {
    pub solution: Solution<E>,
    pub stats: Stats,
}

/// A ring-independent, serializable form of [`Solved`], with polynomials and values in the text
/// syntax of the ring.
#[derive(Clone, Debug, Serialize)]
pub struct Report {
    pub variables: Vec<String>,
    #[serde(flatten)]
    pub outcome: ReportOutcome,
    pub stats: Stats,
}

/// See [`Report`].
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReportOutcome {
    None,
    Finite {
        solutions_with_multiplicity: usize,
        distinct_solutions: usize,
        /// One list of values per solution, in the order of `variables`.
        points: Vec<Vec<String>>,
        all_found: bool,
        basis: Vec<String>,
        lex_basis: Vec<String>,
        note: Option<String>,
    },
    Family {
        dimension: usize,
        independent_variables: Vec<String>,
        basis: Vec<String>,
    },
    LimitReached {
        limit: LimitKind,
        phase: String,
        known: Option<String>,
    },
}

// ---------------------------------------------------------------------------------------------
// Entry points
// ---------------------------------------------------------------------------------------------

/// Solves `equations = 0` over the field of `ring`: see the module documentation.
///
/// The ring must be a commutative polynomial ring (no Laurent variables, at most one odd
/// variable) over a field. Reaching a limit is an answer ([`Solution::LimitReached`]), not an
/// error.
pub fn solve<F: Ring>(
    ring: &PolyRing<F>,
    equations: &[Poly<F::Elem>],
    limits: &Limits,
) -> Result<Solved<F::Elem>> {
    check_ring(ring)?;
    let input = equations
        .iter()
        .map(|p| from_poly(ring, TermOrder::Grevlex, p))
        .collect::<Result<Vec<_>>>()?;
    let mut s = Solver {
        ring,
        work: Work::new(*limits),
        phase: "Gröbner basis",
        known: None,
    };
    let solution = match s.run(input) {
        Ok(sol) => sol,
        Err(Stop(limit)) => Solution::LimitReached {
            limit,
            phase: s.phase.to_string(),
            known: s.known.clone(),
        },
    };
    Ok(Solved {
        solution,
        stats: s.work.stats,
    })
}

/// The reduced Gröbner basis of the ideal generated by `equations`, in `order`.
///
/// Fails with an error of kind [`ErrorKind::Limit`](crate::error::ErrorKind::Limit) when a limit
/// is reached, and with a precondition error unless `ring` is a commutative polynomial ring over
/// a field. The degree limit applies in either order.
pub fn groebner_basis<F: Ring>(
    ring: &PolyRing<F>,
    equations: &[Poly<F::Elem>],
    order: TermOrder,
    limits: &Limits,
) -> Result<Basis<F::Elem>> {
    check_ring(ring)?;
    let input = equations
        .iter()
        .map(|p| from_poly(ring, order, p))
        .collect::<Result<Vec<_>>>()?;
    let mut work = Work::new(*limits);
    match groebner(&ring.base, ring.nvars(), order, input, true, &mut work) {
        Ok(g) => Ok(Basis {
            polys: g.iter().map(|p| to_poly(ring, p)).collect(),
            order,
            stats: work.stats,
        }),
        Err(Stop(limit)) => Err(Error::limit(format!(
            "Gröbner basis: {limit} limit reached ({})",
            fmt_stats(&work.stats)
        ))),
    }
}

fn check_ring<F: Ring>(ring: &PolyRing<F>) -> Result<()> {
    if !ring.base.is_field() {
        return Err(Error::precondition(
            "polynomial solving needs coefficients in a field (QQ or a prime field)",
        ));
    }
    if !ring.is_commutative() {
        return Err(Error::precondition(
            "polynomial solving needs commuting variables",
        ));
    }
    if ring.laurent.iter().any(|&l| l) {
        return Err(Error::precondition(
            "polynomial solving does not handle Laurent variables",
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Work accounting
// ---------------------------------------------------------------------------------------------

/// A computation stopped at a limit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Stop(LimitKind);

type Run<T> = std::result::Result<T, Stop>;

struct Work {
    limits: Limits,
    stats: Stats,
}

impl Work {
    fn new(limits: Limits) -> Self {
        Work {
            limits,
            stats: Stats::default(),
        }
    }

    fn tick(&mut self, n: u64) -> Run<()> {
        self.stats.steps = self.stats.steps.saturating_add(n);
        if self.stats.steps > self.limits.max_steps {
            Err(Stop(LimitKind::Steps))
        } else {
            Ok(())
        }
    }

    fn degree(&mut self, d: u32, check: bool) -> Run<()> {
        let d = d as i64;
        self.stats.max_degree = self.stats.max_degree.max(d);
        if check && d > self.limits.max_degree {
            Err(Stop(LimitKind::Degree))
        } else {
            Ok(())
        }
    }
}

fn fmt_stats(s: &Stats) -> String {
    format!(
        "basis size {}, max degree {}, {} steps, {} S-pairs ({} reduced to zero)",
        s.basis_size, s.max_degree, s.steps, s.pairs, s.zero_reductions
    )
}

// ---------------------------------------------------------------------------------------------
// Internal sparse polynomials
// ---------------------------------------------------------------------------------------------

/// A monomial with its total degree and a divisibility mask (bit `i mod 64` set when `x_i`
/// occurs), so most failed divisibility tests cost one word operation.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Mon {
    deg: u32,
    mask: u64,
    e: SmallVec<[u32; 8]>,
}

impl Mon {
    fn from_exps(e: SmallVec<[u32; 8]>) -> Mon {
        let mut deg = 0;
        let mut mask = 0u64;
        for (i, &x) in e.iter().enumerate() {
            deg += x;
            if x > 0 {
                mask |= 1 << (i % 64);
            }
        }
        Mon { deg, mask, e }
    }

    fn one(n: usize) -> Mon {
        Mon::from_exps(smallvec![0; n])
    }

    fn var(n: usize, i: usize) -> Mon {
        let mut e: SmallVec<[u32; 8]> = smallvec![0; n];
        e[i] = 1;
        Mon::from_exps(e)
    }

    fn divides(&self, o: &Mon) -> bool {
        self.deg <= o.deg
            && self.mask & !o.mask == 0
            && self.e.iter().zip(&o.e).all(|(a, b)| a <= b)
    }

    fn mul(&self, o: &Mon) -> Mon {
        Mon {
            deg: self.deg + o.deg,
            mask: self.mask | o.mask,
            e: self.e.iter().zip(&o.e).map(|(a, b)| a + b).collect(),
        }
    }

    /// `self / o`, for `o` dividing `self`.
    fn div(&self, o: &Mon) -> Mon {
        Mon::from_exps(self.e.iter().zip(&o.e).map(|(a, b)| a - b).collect())
    }

    fn lcm(&self, o: &Mon) -> Mon {
        Mon::from_exps(self.e.iter().zip(&o.e).map(|(a, b)| *a.max(b)).collect())
    }

    fn coprime(&self, o: &Mon) -> bool {
        if self.mask & o.mask == 0 {
            return true;
        }
        self.e.len() > 64 && self.e.iter().zip(&o.e).all(|(a, b)| *a == 0 || *b == 0)
    }

    fn is_one(&self) -> bool {
        self.deg == 0
    }
}

fn cmp_mon(order: TermOrder, a: &Mon, b: &Mon) -> Ordering {
    match order {
        TermOrder::Grevlex => a.deg.cmp(&b.deg).then_with(|| {
            for (x, y) in a.e.iter().zip(&b.e).rev() {
                if x != y {
                    return y.cmp(x);
                }
            }
            Ordering::Equal
        }),
        TermOrder::Lex => a.e.cmp(&b.e),
    }
}

/// A term `(monomial, coefficient)`; a polynomial is a list of terms sorted by decreasing
/// monomial in the order of the computation, with no zero coefficients. Basis elements are
/// monic.
type Term<E> = (Mon, E);
type P<E> = Vec<Term<E>>;

/// Sorts terms by decreasing monomial, merging equal monomials and dropping zeros.
fn normalize<F: Ring>(f: &F, order: TermOrder, mut terms: P<F::Elem>) -> P<F::Elem> {
    terms.sort_by(|a, b| cmp_mon(order, &b.0, &a.0));
    let mut out: P<F::Elem> = Vec::with_capacity(terms.len());
    for (m, c) in terms {
        match out.last_mut() {
            Some((lm, lc)) if *lm == m => f.add_assign(lc, &c),
            _ => out.push((m, c)),
        }
    }
    out.retain(|(_, c)| !f.is_zero(c));
    out
}

fn make_monic<F: Ring>(f: &F, p: &mut P<F::Elem>) {
    if let Some((_, lc)) = p.first()
        && !f.is_one(lc)
    {
        let inv = f.inv(lc).expect("nonzero element of a field is invertible");
        for (_, c) in p.iter_mut() {
            *c = f.mul(c, &inv);
        }
    }
}

fn max_degree<E>(p: &P<E>) -> u32 {
    p.iter().map(|(m, _)| m.deg).max().unwrap_or(0)
}

/// `p + c·m·g`, merged in the term order (`c ≠ 0`).
fn axpy<F: Ring>(
    f: &F,
    order: TermOrder,
    p: &[Term<F::Elem>],
    c: &F::Elem,
    m: &Mon,
    g: &[Term<F::Elem>],
) -> P<F::Elem> {
    let mut out = Vec::with_capacity(p.len() + g.len());
    let mut i = 0;
    let mut j = 0;
    let mut cur = g.first().map(|t| m.mul(&t.0));
    while let Some(gm) = cur.take() {
        if i < p.len() {
            match cmp_mon(order, &p[i].0, &gm) {
                Ordering::Greater => {
                    out.push(p[i].clone());
                    i += 1;
                    cur = Some(gm);
                    continue;
                }
                Ordering::Equal => {
                    let s = f.add(&p[i].1, &f.mul(c, &g[j].1));
                    if !f.is_zero(&s) {
                        out.push((gm, s));
                    }
                    i += 1;
                }
                Ordering::Less => out.push((gm, f.mul(c, &g[j].1))),
            }
        } else {
            out.push((gm, f.mul(c, &g[j].1)));
        }
        j += 1;
        cur = g.get(j).map(|t| m.mul(&t.0));
    }
    out.extend_from_slice(&p[i..]);
    out
}

/// Multiplies by a monomial (the order is preserved).
fn mul_mon<E: Clone>(p: &[Term<E>], m: &Mon) -> P<E> {
    p.iter().map(|(t, c)| (t.mul(m), c.clone())).collect()
}

/// The full normal form of `p` with respect to the monic polynomials `store[k]`, `k ∈ active`:
/// every term divisible by a leading monomial is reduced away, one step per reduction.
fn reduce<F: Ring>(
    f: &F,
    order: TermOrder,
    mut p: P<F::Elem>,
    store: &[P<F::Elem>],
    active: &[usize],
    work: &mut Work,
) -> Run<P<F::Elem>> {
    let mut r = Vec::new();
    let mut start = 0;
    while start < p.len() {
        let m = &p[start].0;
        match active
            .iter()
            .map(|&k| &store[k])
            .find(|g| g[0].0.divides(m))
        {
            Some(g) => {
                work.tick(1)?;
                let q = m.div(&g[0].0);
                let c = f.neg(&p[start].1);
                p = axpy(f, order, &p[start + 1..], &c, &q, &g[1..]);
                start = 0;
            }
            None => {
                r.push(p[start].clone());
                start += 1;
            }
        }
    }
    Ok(r)
}

fn is_unit<E>(g: &[P<E>]) -> bool {
    g.len() == 1 && g[0].len() == 1 && g[0][0].0.is_one()
}

fn from_poly<F: Ring>(
    ring: &PolyRing<F>,
    order: TermOrder,
    p: &Poly<F::Elem>,
) -> Result<P<F::Elem>> {
    let n = ring.nvars();
    let mut terms = Vec::with_capacity(p.terms.len());
    for (m, c) in &p.terms {
        if m.0.len() != n || m.0.iter().any(|&e| e < 0) {
            return Err(Error::invalid(format!(
                "'{}' is not a polynomial of this ring",
                ring.fmt_elem(p)
            )));
        }
        terms.push((
            Mon::from_exps(m.0.iter().map(|&e| e as u32).collect()),
            c.clone(),
        ));
    }
    Ok(normalize(&ring.base, order, terms))
}

fn to_poly<F: Ring>(ring: &PolyRing<F>, p: &P<F::Elem>) -> Poly<F::Elem> {
    ring.normalize(
        p.iter()
            .map(|(m, c)| {
                let e: Exps = m.e.iter().map(|&x| x as i32).collect();
                (Monomial(e), c.clone())
            })
            .collect(),
    )
}

// ---------------------------------------------------------------------------------------------
// Buchberger's algorithm
// ---------------------------------------------------------------------------------------------

/// A critical pair `(i, j)` with the lcm of the leading monomials and its sugar degree.
#[derive(Clone, Debug)]
struct Pair {
    i: usize,
    j: usize,
    lcm: Mon,
    sugar: u32,
}

struct Buchberger<'a, F: Ring> {
    f: &'a F,
    order: TermOrder,
    check_degree: bool,
    /// Every polynomial added so far (pairs refer to them by index).
    polys: Vec<P<F::Elem>>,
    /// Sugar degree of each polynomial: the degree it would have in a homogenized computation.
    sugar: Vec<u32>,
    /// The current minimal basis.
    active: Vec<usize>,
    pairs: Vec<Pair>,
}

impl<F: Ring> Buchberger<'_, F> {
    /// Adds a nonzero reduced polynomial to the basis; true if it is a constant.
    fn insert(&mut self, mut h: P<F::Elem>, sugar: u32, work: &mut Work) -> Run<bool> {
        make_monic(self.f, &mut h);
        if h[0].0.is_one() {
            return Ok(true);
        }
        work.degree(max_degree(&h), self.check_degree)?;
        let k = self.polys.len();
        self.polys.push(h);
        self.sugar.push(sugar);
        self.update(k, work)?;
        work.stats.basis_size = work.stats.basis_size.max(self.active.len());
        if self.active.len() > work.limits.max_basis {
            return Err(Stop(LimitKind::BasisSize));
        }
        Ok(false)
    }

    /// The Gebauer–Möller update (Becker–Weispfenning, *Gröbner Bases*, UPDATE): adds the pairs
    /// of `h` with the basis that survive the chain and product criteria, deletes old pairs made
    /// redundant by `h`, and removes basis elements whose leading monomial `h`'s divides.
    fn update(&mut self, h: usize, work: &mut Work) -> Run<()> {
        work.tick(1 + self.active.len() as u64)?;
        let polys = &self.polys;
        let lm = |k: usize| &polys[k][0].0;
        let lh = lm(h);
        let mut c: Vec<(usize, Mon)> = self.active.iter().map(|&g| (g, lh.lcm(lm(g)))).collect();
        // Chain criterion among the new pairs: drop (h, g) if another new pair's lcm divides
        // its lcm. Pairs with coprime leading monomials are kept here so that they can still
        // discard others, and dropped by the product criterion below.
        let mut d: Vec<(usize, Mon)> = Vec::new();
        while let Some(p) = c.pop() {
            let keep = lh.coprime(lm(p.0))
                || (!c.iter().any(|q| q.1.divides(&p.1)) && !d.iter().any(|q| q.1.divides(&p.1)));
            if keep {
                d.push(p);
            }
        }
        // Chain criterion on the old pairs: (g1, g2) is redundant if LM(h) divides its lcm and
        // the lcms with h are both different from it.
        self.pairs.retain(|pr| {
            !(lh.divides(&pr.lcm) && lh.lcm(lm(pr.i)) != pr.lcm && lh.lcm(lm(pr.j)) != pr.lcm)
        });
        // Product criterion: coprime leading monomials give an S-polynomial reducing to zero.
        for (g, l) in d {
            if lh.coprime(lm(g)) {
                continue;
            }
            let s = (self.sugar[h] - lh.deg).max(self.sugar[g] - lm(g).deg) + l.deg;
            self.pairs.push(Pair {
                i: g,
                j: h,
                lcm: l,
                sugar: s,
            });
        }
        self.active.retain(|&g| !lh.divides(lm(g)));
        self.active.push(h);
        Ok(())
    }

    /// The next pair by the sugar strategy: least sugar, then least lcm.
    fn select(&self) -> Option<usize> {
        let order = self.order;
        (0..self.pairs.len()).min_by(|&a, &b| {
            let (p, q) = (&self.pairs[a], &self.pairs[b]);
            p.sugar
                .cmp(&q.sugar)
                .then_with(|| cmp_mon(order, &p.lcm, &q.lcm))
                .then_with(|| (p.i, p.j).cmp(&(q.i, q.j)))
        })
    }

    /// The S-polynomial `(L/LM(g_i))·g_i − (L/LM(g_j))·g_j` of monic `g_i`, `g_j`, `L` their lcm.
    fn spoly(&self, pair: &Pair) -> P<F::Elem> {
        let (gi, gj) = (&self.polys[pair.i], &self.polys[pair.j]);
        let a = mul_mon(&gi[1..], &pair.lcm.div(&gi[0].0));
        let minus_one = self.f.neg(&self.f.one());
        axpy(
            self.f,
            self.order,
            &a,
            &minus_one,
            &pair.lcm.div(&gj[0].0),
            &gj[1..],
        )
    }
}

/// The reduced Gröbner basis of the ideal generated by `input`, sorted by increasing leading
/// monomial; `[1]` for the unit ideal and `[]` for the zero ideal.
fn groebner<F: Ring>(
    f: &F,
    n: usize,
    order: TermOrder,
    input: Vec<P<F::Elem>>,
    check_degree: bool,
    work: &mut Work,
) -> Run<Vec<P<F::Elem>>> {
    let unit = || vec![vec![(Mon::one(n), f.one())]];
    let mut b = Buchberger {
        f,
        order,
        check_degree,
        polys: Vec::new(),
        sugar: Vec::new(),
        active: Vec::new(),
        pairs: Vec::new(),
    };
    let mut input: Vec<P<F::Elem>> = input.into_iter().filter(|p| !p.is_empty()).collect();
    input.sort_by(|a, b| cmp_mon(order, &a[0].0, &b[0].0));
    for p in input {
        work.tick(1)?;
        let s = max_degree(&p);
        work.degree(s, check_degree)?;
        let h = reduce(f, order, p, &b.polys, &b.active, work)?;
        if !h.is_empty() && b.insert(h, s, work)? {
            return Ok(unit());
        }
    }
    while let Some(k) = b.select() {
        let pair = b.pairs.swap_remove(k);
        work.tick(1)?;
        work.stats.pairs += 1;
        work.degree(pair.lcm.deg, check_degree)?;
        let s = b.spoly(&pair);
        let h = reduce(f, order, s, &b.polys, &b.active, work)?;
        if h.is_empty() {
            work.stats.zero_reductions += 1;
        } else if b.insert(h, pair.sugar, work)? {
            return Ok(unit());
        }
    }
    // Interreduce the minimal basis: reduce each tail by the others. Leading monomials do not
    // divide each other, so one pass leaves every tail standard.
    let mut g: Vec<P<F::Elem>> = b.active.iter().map(|&k| b.polys[k].clone()).collect();
    g.sort_by(|a, b| cmp_mon(order, &a[0].0, &b[0].0));
    for k in 0..g.len() {
        let others: Vec<usize> = (0..g.len()).filter(|&j| j != k).collect();
        let p = std::mem::take(&mut g[k]);
        g[k] = reduce(f, order, p, &g, &others, work)?;
    }
    Ok(g)
}

// ---------------------------------------------------------------------------------------------
// Dimension and standard monomials
// ---------------------------------------------------------------------------------------------

/// A largest set `S` of variables such that no leading monomial has support inside `S`: the
/// Krull dimension of the ideal is `|S|`. Branch and bound over include/exclude decisions.
fn max_independent(n: usize, lms: &[Mon], work: &mut Work) -> Run<Vec<usize>> {
    struct Search<'a> {
        n: usize,
        supports: Vec<Vec<usize>>,
        in_set: Vec<bool>,
        cur: Vec<usize>,
        best: Vec<usize>,
        work: &'a mut Work,
    }
    impl Search<'_> {
        fn go(&mut self, i: usize) -> Run<()> {
            self.work.tick(1)?;
            if self.cur.len() + (self.n - i) <= self.best.len() {
                return Ok(());
            }
            if i == self.n {
                self.best = self.cur.clone();
                return Ok(());
            }
            self.in_set[i] = true;
            let ok = self
                .supports
                .iter()
                .all(|s| !s.contains(&i) || !s.iter().all(|&j| self.in_set[j]));
            if ok {
                self.cur.push(i);
                self.go(i + 1)?;
                self.cur.pop();
            }
            self.in_set[i] = false;
            self.go(i + 1)
        }
    }
    let mut s = Search {
        n,
        supports: lms
            .iter()
            .map(|m| (0..n).filter(|&i| m.e[i] > 0).collect())
            .collect(),
        in_set: vec![false; n],
        cur: Vec::new(),
        best: Vec::new(),
        work,
    };
    s.go(0)?;
    Ok(s.best)
}

/// The monomials divisible by no leading monomial (finitely many: the ideal is
/// zero-dimensional), in increasing order.
fn standard_monomials(n: usize, order: TermOrder, lms: &[Mon], work: &mut Work) -> Run<Vec<Mon>> {
    let mut seen: HashSet<SmallVec<[u32; 8]>> = HashSet::new();
    let mut queue = VecDeque::from([Mon::one(n)]);
    seen.insert(queue[0].e.clone());
    let mut out = Vec::new();
    while let Some(m) = queue.pop_front() {
        if lms.iter().any(|l| l.divides(&m)) {
            continue;
        }
        work.tick(1)?;
        for i in 0..n {
            let m2 = m.mul(&Mon::var(n, i));
            if seen.insert(m2.e.clone()) {
                queue.push_back(m2);
            }
        }
        out.push(m);
    }
    out.sort_by(|a, b| cmp_mon(order, a, b));
    Ok(out)
}

// ---------------------------------------------------------------------------------------------
// Linear algebra in the quotient F[x]/I
// ---------------------------------------------------------------------------------------------

/// A row of [`Echelon`]: `v[pivot] = 1`, `v` vanishes at the pivots of earlier rows, and
/// `v = Σ_k c[k] · (inserted vector k)`.
struct Row<E> {
    pivot: usize,
    v: Vec<E>,
    c: Vec<E>,
}

/// Incremental Gaussian elimination that remembers how each row was combined from the inserted
/// vectors, to read off linear relations.
struct Echelon<E> {
    rows: Vec<Row<E>>,
}

impl<E: Clone> Echelon<E> {
    fn new() -> Self {
        Echelon { rows: Vec::new() }
    }

    /// Reduces `v` by the rows: returns `w` and `c` with `w = v − Σ_k c[k] · (vector k)`.
    fn reduce<F: Ring<Elem = E>>(
        &self,
        f: &F,
        mut v: Vec<E>,
        work: &mut Work,
    ) -> Run<(Vec<E>, Vec<E>)> {
        let mut comb: Vec<E> = Vec::new();
        for row in &self.rows {
            if f.is_zero(&v[row.pivot]) {
                continue;
            }
            work.tick(1)?;
            let fac = v[row.pivot].clone();
            for (x, y) in v.iter_mut().zip(&row.v) {
                if !f.is_zero(y) {
                    *x = f.sub(x, &f.mul(&fac, y));
                }
            }
            if comb.len() < row.c.len() {
                comb.resize(row.c.len(), f.zero());
            }
            for (x, y) in comb.iter_mut().zip(&row.c) {
                if !f.is_zero(y) {
                    *x = f.add(x, &f.mul(&fac, y));
                }
            }
        }
        Ok((v, comb))
    }

    /// Adds the nonzero residue `w = (vector index) − Σ_k comb[k] · (vector k)` as a row, where
    /// `index` is the number of vectors inserted before.
    fn push<F: Ring<Elem = E>>(&mut self, f: &F, w: Vec<E>, comb: Vec<E>, index: usize) {
        let pivot = w.iter().position(|x| !f.is_zero(x)).expect("nonzero row");
        let inv = f
            .inv(&w[pivot])
            .expect("nonzero element of a field is invertible");
        let v: Vec<E> = w.iter().map(|x| f.mul(x, &inv)).collect();
        let mut c: Vec<E> = comb.iter().map(|x| f.neg(&f.mul(x, &inv))).collect();
        c.resize(index, f.zero());
        c.push(inv);
        self.rows.push(Row { pivot, v, c });
    }
}

/// Normal forms in `F[x]/I` as coordinate vectors over the standard monomials.
struct Quotient<'a, F: Ring> {
    f: &'a F,
    n: usize,
    basis: &'a [P<F::Elem>],
    all: Vec<usize>,
    index: HashMap<Mon, usize>,
}

impl<'a, F: Ring> Quotient<'a, F> {
    fn new(f: &'a F, n: usize, basis: &'a [P<F::Elem>], std: &[Mon]) -> Self {
        Quotient {
            f,
            n,
            basis,
            all: (0..basis.len()).collect(),
            index: std
                .iter()
                .cloned()
                .enumerate()
                .map(|(i, m)| (m, i))
                .collect(),
        }
    }

    fn nf(&self, p: P<F::Elem>, work: &mut Work) -> Run<P<F::Elem>> {
        reduce(self.f, TermOrder::Grevlex, p, self.basis, &self.all, work)
    }

    fn one(&self, work: &mut Work) -> Run<P<F::Elem>> {
        self.nf(vec![(Mon::one(self.n), self.f.one())], work)
    }

    /// The normal form of `x_i · p` for a normal form `p`.
    fn times_var(&self, p: &P<F::Elem>, i: usize, work: &mut Work) -> Run<P<F::Elem>> {
        self.nf(mul_mon(p, &Mon::var(self.n, i)), work)
    }

    fn vector(&self, p: &P<F::Elem>) -> Vec<F::Elem> {
        let mut v = vec![self.f.zero(); self.index.len()];
        for (m, c) in p {
            v[self.index[m]] = c.clone();
        }
        v
    }

    /// The minimal polynomial of multiplication by `x_i`, as coefficients from degree 0 up.
    fn minpoly(&self, i: usize, work: &mut Work) -> Run<Vec<F::Elem>> {
        let f = self.f;
        let mut ech = Echelon::new();
        let mut cur = self.one(work)?;
        let mut k = 0;
        loop {
            let (w, comb) = ech.reduce(f, self.vector(&cur), work)?;
            if w.iter().all(|x| f.is_zero(x)) {
                let mut u: Vec<F::Elem> = comb.iter().map(|c| f.neg(c)).collect();
                u.resize(k, f.zero());
                u.push(f.one());
                return Ok(u);
            }
            ech.push(f, w, comb, k);
            k += 1;
            cur = self.times_var(&cur, i, work)?;
        }
    }

    /// FGLM: the reduced lexicographic Gröbner basis of `I`, from the grevlex one. Monomials are
    /// visited in increasing lex order; each is either a new standard monomial (its normal form
    /// is independent of the earlier ones) or the leading monomial of a new basis element (the
    /// linear relation).
    fn fglm(&self, work: &mut Work) -> Run<Vec<P<F::Elem>>> {
        let (f, n) = (self.f, self.n);
        let mut ech = Echelon::new();
        let mut stair: Vec<Mon> = Vec::new();
        let mut stair_nf: Vec<P<F::Elem>> = Vec::new();
        let mut lex: Vec<P<F::Elem>> = Vec::new();
        // Candidates in lex order, each with a parent `(staircase index, variable)`.
        let mut cand: BTreeMap<SmallVec<[u32; 8]>, Option<(usize, usize)>> = BTreeMap::new();
        cand.insert(smallvec![0; n], None);
        while let Some((e, parent)) = cand.pop_first() {
            let t = Mon::from_exps(e);
            if lex.iter().any(|g| g[0].0.divides(&t)) {
                continue;
            }
            work.tick(1)?;
            let nf = match parent {
                None => self.one(work)?,
                Some((b, i)) => self.times_var(&stair_nf[b], i, work)?,
            };
            let (w, comb) = ech.reduce(f, self.vector(&nf), work)?;
            if w.iter().all(|x| f.is_zero(x)) {
                let mut terms = vec![(t, f.one())];
                for (k, c) in comb.iter().enumerate() {
                    if !f.is_zero(c) {
                        terms.push((stair[k].clone(), f.neg(c)));
                    }
                }
                lex.push(normalize(f, TermOrder::Lex, terms));
            } else {
                ech.push(f, w, comb, stair.len());
                let k = stair.len();
                for i in 0..n {
                    let mut e2 = t.e.clone();
                    e2[i] += 1;
                    cand.entry(e2).or_insert(Some((k, i)));
                }
                stair.push(t);
                stair_nf.push(nf);
            }
        }
        lex.sort_by(|a, b| cmp_mon(TermOrder::Lex, &a[0].0, &b[0].0));
        Ok(lex)
    }
}

// ---------------------------------------------------------------------------------------------
// Univariate polynomials (dense, coefficients from degree 0 up, no trailing zeros)
// ---------------------------------------------------------------------------------------------

fn u_trim<F: Ring>(f: &F, mut a: Vec<F::Elem>) -> Vec<F::Elem> {
    while a.last().is_some_and(|c| f.is_zero(c)) {
        a.pop();
    }
    a
}

fn u_monic<F: Ring>(f: &F, a: Vec<F::Elem>) -> Vec<F::Elem> {
    let a = u_trim(f, a);
    match a.last() {
        Some(lc) if !f.is_one(lc) => {
            let inv = f.inv(lc).expect("nonzero element of a field is invertible");
            a.iter().map(|c| f.mul(c, &inv)).collect()
        }
        _ => a,
    }
}

fn u_sub<F: Ring>(f: &F, a: &[F::Elem], b: &[F::Elem]) -> Vec<F::Elem> {
    let n = a.len().max(b.len());
    let z = f.zero();
    let r = (0..n)
        .map(|i| f.sub(a.get(i).unwrap_or(&z), b.get(i).unwrap_or(&z)))
        .collect();
    u_trim(f, r)
}

fn u_mul<F: Ring>(f: &F, a: &[F::Elem], b: &[F::Elem]) -> Vec<F::Elem> {
    if a.is_empty() || b.is_empty() {
        return Vec::new();
    }
    let mut r = vec![f.zero(); a.len() + b.len() - 1];
    for (i, x) in a.iter().enumerate() {
        if f.is_zero(x) {
            continue;
        }
        for (j, y) in b.iter().enumerate() {
            r[i + j] = f.add(&r[i + j], &f.mul(x, y));
        }
    }
    u_trim(f, r)
}

fn u_deriv<F: Ring>(f: &F, a: &[F::Elem]) -> Vec<F::Elem> {
    let r = a
        .iter()
        .enumerate()
        .skip(1)
        .map(|(i, c)| f.mul(&f.int(i as i64), c))
        .collect();
    u_trim(f, r)
}

fn u_eval<F: Ring>(f: &F, a: &[F::Elem], x: &F::Elem) -> F::Elem {
    a.iter()
        .rev()
        .fold(f.zero(), |acc, c| f.add(&f.mul(&acc, x), c))
}

/// A quotient and a remainder.
type QuoRem<E> = (Vec<E>, Vec<E>);

/// Division with remainder by a nonzero `b` (one step per quotient term).
fn u_divrem<F: Ring>(f: &F, a: &[F::Elem], b: &[F::Elem], work: &mut Work) -> Run<QuoRem<F::Elem>> {
    let mut r = u_trim(f, a.to_vec());
    let db = b.len() - 1;
    if r.len() < b.len() {
        return Ok((Vec::new(), r));
    }
    let inv = f
        .inv(&b[db])
        .expect("nonzero element of a field is invertible");
    let mut q = vec![f.zero(); r.len() - db];
    while r.len() > db {
        work.tick(1)?;
        let k = r.len() - 1 - db;
        let c = f.mul(&r[r.len() - 1], &inv);
        for (j, y) in b.iter().enumerate() {
            r[k + j] = f.sub(&r[k + j], &f.mul(&c, y));
        }
        q[k] = c;
        r.pop();
        r = u_trim(f, r);
    }
    Ok((u_trim(f, q), r))
}

/// The monic gcd (empty if both are zero).
fn u_gcd<F: Ring>(f: &F, a: &[F::Elem], b: &[F::Elem], work: &mut Work) -> Run<Vec<F::Elem>> {
    let (mut a, mut b) = (u_trim(f, a.to_vec()), u_trim(f, b.to_vec()));
    while !b.is_empty() {
        let r = u_monic(f, u_divrem(f, &a, &b, work)?.1);
        a = b;
        b = r;
    }
    Ok(u_monic(f, a))
}

/// `base^e mod m` for `deg m ≥ 1`.
fn u_powmod<F: Ring>(
    f: &F,
    base: &[F::Elem],
    mut e: u64,
    m: &[F::Elem],
    work: &mut Work,
) -> Run<Vec<F::Elem>> {
    let mut acc = vec![f.one()];
    let mut b = u_divrem(f, base, m, work)?.1;
    while e > 0 {
        if e & 1 == 1 {
            acc = u_divrem(f, &u_mul(f, &acc, &b), m, work)?.1;
        }
        e >>= 1;
        if e > 0 {
            b = u_divrem(f, &u_mul(f, &b, &b), m, work)?.1;
        }
    }
    Ok(acc)
}

/// The square-free part (product of the distinct monic irreducible factors).
///
/// In characteristic 0 this is `a / gcd(a, a')`. In characteristic `p`, `w = a / gcd(a, a')`
/// collects the factors of multiplicity prime to `p` and the others are found recursively in
/// `gcd(a, a')`; when `a' = 0`, `a(x) = b(x^p) = b(x)^p` over a prime field (Frobenius is the
/// identity on ℤ/p).
fn u_sqfree<F: Ring>(f: &F, a: &[F::Elem], work: &mut Work) -> Run<Vec<F::Elem>> {
    let a = u_monic(f, a.to_vec());
    if a.len() <= 2 {
        return Ok(a);
    }
    let d = u_deriv(f, &a);
    if d.is_empty() {
        let p = f.characteristic() as usize;
        let b: Vec<F::Elem> = a.iter().step_by(p.max(1)).cloned().collect();
        return u_sqfree(f, &b, work);
    }
    let g = u_gcd(f, &a, &d, work)?;
    let w = u_monic(f, u_divrem(f, &a, &g, work)?.0);
    if g.len() <= 1 || f.characteristic() == 0 {
        return Ok(w);
    }
    let rg = u_sqfree(f, &g, work)?;
    let gg = u_gcd(f, &w, &rg, work)?;
    Ok(u_monic(f, u_divrem(f, &u_mul(f, &w, &rg), &gg, work)?.0))
}

// ---------------------------------------------------------------------------------------------
// Roots in the prime subfield
// ---------------------------------------------------------------------------------------------

/// Root finding stopped: at a limit, or because the polynomial is not over the prime field.
enum RootFail {
    Stop(Stop),
    Unsupported(String),
}

impl From<Stop> for RootFail {
    fn from(s: Stop) -> Self {
        RootFail::Stop(s)
    }
}

/// Primes up to this size are searched for roots by evaluation at every element.
const BRUTE_FORCE_PRIME: u64 = 4096;

/// The distinct roots of `u` in the prime subfield of `f`.
fn roots<F: Ring>(
    f: &F,
    u: &[F::Elem],
    work: &mut Work,
) -> std::result::Result<Vec<F::Elem>, RootFail> {
    let u = u_monic(f, u.to_vec());
    if u.len() <= 1 {
        return Ok(Vec::new());
    }
    match f.characteristic() {
        0 => rational_roots(f, &u, work),
        p => Ok(prime_field_roots(f, p, &u, work)?),
    }
}

fn prime_field_roots<F: Ring>(f: &F, p: u64, u: &[F::Elem], work: &mut Work) -> Run<Vec<F::Elem>> {
    if p <= BRUTE_FORCE_PRIME {
        let mut out = Vec::new();
        for k in 0..p {
            work.tick(1)?;
            let x = f.int(k as i64);
            if f.is_zero(&u_eval(f, u, &x)) {
                out.push(x);
            }
        }
        return Ok(out);
    }
    // h = gcd(u, x^p − x) is the product of the distinct linear factors of u over ℤ/p.
    let x = vec![f.zero(), f.one()];
    let xp = u_powmod(f, &x, p, u, work)?;
    let h = u_gcd(f, u, &u_sub(f, &xp, &x), work)?;
    let mut out = Vec::new();
    split_linear(f, p, h, work, &mut out)?;
    Ok(out)
}

/// Splits a monic product of distinct linear factors over ℤ/p (`p` odd) by Cantor–Zassenhaus
/// with the deterministic shifts `a = 0, 1, 2, …`: `gcd(h, (x + a)^((p−1)/2) − 1)` collects the
/// roots `r` with `r + a` a nonzero square.
fn split_linear<F: Ring>(
    f: &F,
    p: u64,
    h: Vec<F::Elem>,
    work: &mut Work,
    out: &mut Vec<F::Elem>,
) -> Run<()> {
    match h.len() {
        0 | 1 => return Ok(()),
        2 => {
            out.push(f.neg(&h[0]));
            return Ok(());
        }
        _ => {}
    }
    for a in 0..p {
        work.tick(1)?;
        let base = vec![f.int(a as i64), f.one()];
        let t = u_sub(f, &u_powmod(f, &base, (p - 1) / 2, &h, work)?, &[f.one()]);
        let g = u_gcd(f, &h, &t, work)?;
        if g.len() > 1 && g.len() < h.len() {
            let q = u_monic(f, u_divrem(f, &h, &g, work)?.0);
            split_linear(f, p, g, work, out)?;
            return split_linear(f, p, q, work, out);
        }
    }
    Ok(())
}

/// `c` as a fraction `n/d`, read from its canonical text form and checked by mapping it back.
fn as_rational<F: Ring>(f: &F, c: &F::Elem) -> Option<(BigInt, BigInt)> {
    let s = f.fmt_elem(c);
    let (n, d): (BigInt, BigInt) = match s.split_once('/') {
        Some((a, b)) => (a.trim().parse().ok()?, b.trim().parse().ok()?),
        None => (s.trim().parse().ok()?, BigInt::one()),
    };
    (f.rational(&n, &d).as_ref() == Some(c)).then_some((n, d))
}

/// The rational roots of `u`, by the rational root theorem: a root `p/q` in lowest terms of a
/// primitive integer polynomial `Σ a_i x^i` of degree `d` with `a_0 ≠ 0` has `p | a_0` and
/// `q | a_d`. The square-free part is not taken first: Euclid's algorithm over ℚ on a large
/// eliminant can cost far more than the enumeration it would shorten.
fn rational_roots<F: Ring>(
    f: &F,
    u: &[F::Elem],
    work: &mut Work,
) -> std::result::Result<Vec<F::Elem>, RootFail> {
    let mut rats = Vec::with_capacity(u.len());
    for c in u {
        rats.push(as_rational(f, c).ok_or_else(|| {
            RootFail::Unsupported(format!(
                "the coefficient {} is not a rational number, so roots were not searched",
                f.fmt_elem(c)
            ))
        })?);
    }
    let l = rats.iter().fold(BigInt::one(), |acc, (_, d)| acc.lcm(d));
    let mut a: Vec<BigInt> = rats.iter().map(|(n, d)| n * (&l / d)).collect();
    let g = a.iter().fold(BigInt::zero(), |acc, x| acc.gcd(x));
    for x in &mut a {
        *x /= &g;
    }
    let mut found: Vec<(BigInt, BigInt)> = Vec::new();
    let zeros = a.iter().take_while(|x| x.is_zero()).count();
    if zeros > 0 {
        found.push((BigInt::zero(), BigInt::one()));
        a.drain(..zeros);
    }
    match a.len() - 1 {
        0 => {}
        1 => found.push((-&a[0], a[1].clone())),
        2 => {
            let disc = &a[1] * &a[1] - BigInt::from(4) * &a[2] * &a[0];
            if !disc.is_negative() {
                let r = disc.sqrt();
                if &r * &r == disc {
                    let two_a = BigInt::from(2) * &a[2];
                    found.push((-&a[1] + &r, two_a.clone()));
                    found.push((-&a[1] - &r, two_a));
                }
            }
        }
        d => {
            let ps = divisors(&a[0].abs(), work)?;
            let qs = divisors(&a[d].abs(), work)?;
            for q in &qs {
                for p in &ps {
                    if !p.gcd(q).is_one() {
                        continue;
                    }
                    for p in [p.clone(), -p] {
                        work.tick(1)?;
                        if hom_eval(&a, &p, q).is_zero() {
                            found.push((p, q.clone()));
                        }
                    }
                }
            }
        }
    }
    let mut out: Vec<F::Elem> = Vec::new();
    for x in found.iter().filter_map(|(p, q)| f.rational(p, q)) {
        if f.is_zero(&u_eval(f, u, &x)) && !out.contains(&x) {
            out.push(x);
        }
    }
    Ok(out)
}

/// `q^d · a(p/q)` for `a` of degree `d`.
fn hom_eval(a: &[BigInt], p: &BigInt, q: &BigInt) -> BigInt {
    let d = a.len() - 1;
    let mut v = a[d].clone();
    let mut qp = BigInt::one();
    for i in (0..d).rev() {
        qp *= q;
        v = v * p + &a[i] * &qp;
    }
    v
}

/// The positive divisors of `n > 0`, by trial division (one step per trial and per divisor).
fn divisors(n: &BigInt, work: &mut Work) -> Run<Vec<BigInt>> {
    let mut m = n.clone();
    let mut factors: Vec<(BigInt, u32)> = Vec::new();
    let two = BigInt::from(2);
    let mut d = two.clone();
    while &d * &d <= m {
        work.tick(1)?;
        let mut e = 0;
        while (&m % &d).is_zero() {
            m /= &d;
            e += 1;
        }
        if e > 0 {
            factors.push((d.clone(), e));
        }
        d += if d == two { 1 } else { 2 };
    }
    if m > BigInt::one() {
        factors.push((m, 1));
    }
    let mut divs = vec![BigInt::one()];
    for (p, e) in factors {
        let mut next = Vec::new();
        for x in &divs {
            let mut y = x.clone();
            for _ in 0..=e {
                work.tick(1)?;
                next.push(y.clone());
                y *= &p;
            }
        }
        divs = next;
    }
    Ok(divs)
}

// ---------------------------------------------------------------------------------------------
// Solving
// ---------------------------------------------------------------------------------------------

struct Solver<'a, F: Ring> {
    ring: &'a PolyRing<F>,
    work: Work,
    phase: &'static str,
    known: Option<String>,
}

impl<F: Ring> Solver<'_, F> {
    fn run(&mut self, input: Vec<P<F::Elem>>) -> Run<Solution<F::Elem>> {
        let (ring, f, n) = (self.ring, &self.ring.base, self.ring.nvars());
        let g = groebner(f, n, TermOrder::Grevlex, input, true, &mut self.work)?;
        if is_unit(&g) {
            return Ok(Solution::None);
        }
        let to_polys = |g: &[P<F::Elem>]| g.iter().map(|p| to_poly(ring, p)).collect::<Vec<_>>();
        let lms: Vec<Mon> = g.iter().map(|p| p[0].0.clone()).collect();
        self.phase = "dimension";
        let independent = max_independent(n, &lms, &mut self.work)?;
        if !independent.is_empty() {
            return Ok(Solution::Family(Family {
                dimension: independent.len(),
                independent,
                basis: to_polys(&g),
            }));
        }
        self.phase = "standard monomials";
        self.known = Some("zero-dimensional".into());
        let std = standard_monomials(n, TermOrder::Grevlex, &lms, &mut self.work)?;
        let with_multiplicity = std.len();
        self.known = Some(format!(
            "zero-dimensional, {with_multiplicity} solutions over the algebraic closure counted \
             with multiplicity"
        ));
        let q = Quotient::new(f, n, &g, &std);
        self.phase = "distinct solutions (radical)";
        let distinct = self.count_distinct(&q, &g, with_multiplicity)?;
        self.known = Some(format!(
            "zero-dimensional, {with_multiplicity} solutions over the algebraic closure counted \
             with multiplicity, {distinct} distinct"
        ));
        self.phase = "lex basis (FGLM)";
        let lex = q.fglm(&mut self.work)?;
        self.phase = "solutions in the field";
        let mut search = PointSearch {
            f,
            lex: &lex,
            work: &mut self.work,
            point: vec![None; n],
            points: Vec::new(),
            note: None,
        };
        search.go(n)?;
        let (points, note) = (search.points, search.note);
        let all_found = note.is_none() && points.len() == distinct;
        Ok(Solution::Finite(FiniteSolutions {
            with_multiplicity,
            distinct,
            points,
            all_found,
            basis: to_polys(&g),
            lex_basis: to_polys(&lex),
            note,
        }))
    }

    /// `dim F[x]/√I`, with `√I = I + (sqfree(m_1)(x_1), …, sqfree(m_n)(x_n))` for the minimal
    /// polynomials `m_i` of the variables on `F[x]/I` (Seidenberg's lemma; the fields here are
    /// perfect, so square-free polynomials are separable).
    fn count_distinct(
        &mut self,
        q: &Quotient<'_, F>,
        g: &[P<F::Elem>],
        with_multiplicity: usize,
    ) -> Run<usize> {
        let (f, n) = (q.f, q.n);
        let mut extra = Vec::new();
        for i in 0..n {
            let m = q.minpoly(i, &mut self.work)?;
            let r = u_sqfree(f, &m, &mut self.work)?;
            if r.len() < m.len() {
                extra.push(from_univariate(f, n, i, &r));
            }
        }
        if extra.is_empty() {
            return Ok(with_multiplicity);
        }
        let mut input = g.to_vec();
        input.extend(extra);
        let g2 = groebner(f, n, TermOrder::Grevlex, input, false, &mut self.work)?;
        let lms: Vec<Mon> = g2.iter().map(|p| p[0].0.clone()).collect();
        Ok(standard_monomials(n, TermOrder::Grevlex, &lms, &mut self.work)?.len())
    }
}

/// `Σ u_k x_i^k` as a sparse polynomial (the same in every order).
fn from_univariate<F: Ring>(f: &F, n: usize, i: usize, u: &[F::Elem]) -> P<F::Elem> {
    u.iter()
        .enumerate()
        .rev()
        .filter(|(_, c)| !f.is_zero(c))
        .map(|(k, c)| {
            let mut e: SmallVec<[u32; 8]> = smallvec![0; n];
            e[i] = k as u32;
            (Mon::from_exps(e), c.clone())
        })
        .collect()
}

/// Back-substitution through the reduced lex basis `G`. The elimination ideal
/// `I_k = I ∩ F[x_k, …, x_n]` is generated by `G_k = G ∩ F[x_k, …, x_n]`, so a point
/// `a = (a_{k+1}, …, a_n)` of `V(I_{k+1})` extends to `(b, a) ∈ V(I_k)` exactly when `b` is a
/// common root of the specializations `h(x_k, a)`, `h ∈ G_k` (those not involving `x_k` vanish
/// at `a`): the roots of their gcd. Each extension in the field is followed to the next
/// variable. For zero-dimensional `I`, `G_k` contains a polynomial with leading monomial a power
/// of `x_k`, which stays monic under specialization, so the gcd is nonzero.
struct PointSearch<'a, F: Ring> {
    f: &'a F,
    lex: &'a [P<F::Elem>],
    work: &'a mut Work,
    point: Vec<Option<F::Elem>>,
    points: Vec<Vec<F::Elem>>,
    note: Option<String>,
}

impl<F: Ring> PointSearch<'_, F> {
    /// Searches with the variables `x_{k+1}, …, x_n` (0-based indices `k, …`) fixed.
    fn go(&mut self, k: usize) -> Run<()> {
        let f = self.f;
        let Some(k) = k.checked_sub(1) else {
            let pt = self.point.iter().map(|x| x.clone().expect("fixed"));
            self.points.push(pt.collect());
            return Ok(());
        };
        let mut u: Vec<F::Elem> = Vec::new();
        for h in self.lex {
            let lm = &h[0].0;
            if lm.e[k] == 0 || lm.e[..k].iter().any(|&x| x > 0) {
                continue;
            }
            self.work.tick(1)?;
            u = u_gcd(f, &u, &self.specialize(h, k), self.work)?;
            if u.len() == 1 {
                return Ok(());
            }
        }
        let rs = match roots(f, &u, self.work) {
            Ok(rs) => rs,
            Err(RootFail::Stop(s)) => return Err(s),
            Err(RootFail::Unsupported(msg)) => {
                self.note = Some(msg);
                return Ok(());
            }
        };
        for a in rs {
            self.point[k] = Some(a);
            self.go(k)?;
            self.point[k] = None;
        }
        Ok(())
    }

    /// `h(x_k, a_{k+1}, …, a_n)` as a univariate polynomial in `x_k`.
    fn specialize(&self, h: &P<F::Elem>, k: usize) -> Vec<F::Elem> {
        let f = self.f;
        let mut u = vec![f.zero(); h[0].0.e[k] as usize + 1];
        for (m, c) in h {
            let mut t = c.clone();
            for (j, &e) in m.e.iter().enumerate().skip(k + 1) {
                if e > 0 {
                    let a = self.point[j].as_ref().expect("later variables are fixed");
                    t = f.mul(&t, &f.pow(a, e));
                }
            }
            let d = m.e[k] as usize;
            u[d] = f.add(&u[d], &t);
        }
        u_trim(f, u)
    }
}

// ---------------------------------------------------------------------------------------------
// Output
// ---------------------------------------------------------------------------------------------

impl<E: Clone> Solved<E> {
    /// The answer as text, with polynomials and values in the syntax of `ring`.
    pub fn fmt<F: Ring<Elem = E>>(&self, ring: &PolyRing<F>) -> String {
        let polys = |ps: &[Poly<E>]| -> String {
            ps.iter()
                .map(|p| format!("\n  {}", ring.fmt_elem(p)))
                .collect()
        };
        let mut s = match &self.solution {
            Solution::None => {
                "no solutions: the equations generate the unit ideal (Gröbner basis {1})".into()
            }
            Solution::Finite(fs) => {
                let mut s = format!(
                    "finitely many solutions: {} over the algebraic closure ({} counted with \
                     multiplicity), {} with coordinates in the base field",
                    fs.distinct,
                    fs.with_multiplicity,
                    fs.points.len()
                );
                if fs.all_found {
                    s.push_str(" (all of them)");
                }
                if !fs.points.is_empty() {
                    s.push(':');
                }
                for pt in &fs.points {
                    s.push_str("\n  ");
                    if pt.is_empty() {
                        s.push_str("(the empty point)");
                    }
                    let vals: Vec<String> = pt
                        .iter()
                        .zip(&ring.names)
                        .map(|(v, x)| format!("{x} = {}", ring.base.fmt_elem(v)))
                        .collect();
                    s.push_str(&vals.join(", "));
                }
                if let Some(note) = &fs.note {
                    s.push_str(&format!("\nincomplete search: {note}"));
                }
                if !fs.all_found {
                    s.push_str("\nall solutions are the common zeros of the lex Gröbner basis:");
                    s.push_str(&polys(&fs.lex_basis));
                }
                s
            }
            Solution::Family(fam) => {
                let vars: Vec<&str> = fam
                    .independent
                    .iter()
                    .map(|&i| ring.names[i].as_str())
                    .collect();
                let mut s = format!(
                    "positive-dimensional: dimension {} (independent variables: {})",
                    fam.dimension,
                    vars.join(", ")
                );
                if fam.basis.is_empty() {
                    s.push_str("\nevery point is a solution");
                } else {
                    s.push_str("\nreduced Gröbner basis (grevlex):");
                    s.push_str(&polys(&fam.basis));
                }
                s
            }
            Solution::LimitReached {
                limit,
                phase,
                known,
            } => {
                let mut s = format!("limit reached: the {limit} limit, during {phase}");
                if let Some(k) = known {
                    s.push_str(&format!("\nknown so far: {k}"));
                }
                s
            }
        };
        s.push_str(&format!("\n({})", fmt_stats(&self.stats)));
        s
    }

    /// The serializable form of the answer.
    pub fn report<F: Ring<Elem = E>>(&self, ring: &PolyRing<F>) -> Report {
        let polys = |ps: &[Poly<E>]| ps.iter().map(|p| ring.fmt_elem(p)).collect();
        let outcome = match &self.solution {
            Solution::None => ReportOutcome::None,
            Solution::Finite(fs) => ReportOutcome::Finite {
                solutions_with_multiplicity: fs.with_multiplicity,
                distinct_solutions: fs.distinct,
                points: fs
                    .points
                    .iter()
                    .map(|pt| pt.iter().map(|v| ring.base.fmt_elem(v)).collect())
                    .collect(),
                all_found: fs.all_found,
                basis: polys(&fs.basis),
                lex_basis: polys(&fs.lex_basis),
                note: fs.note.clone(),
            },
            Solution::Family(fam) => ReportOutcome::Family {
                dimension: fam.dimension,
                independent_variables: fam
                    .independent
                    .iter()
                    .map(|&i| ring.names[i].clone())
                    .collect(),
                basis: polys(&fam.basis),
            },
            Solution::LimitReached {
                limit,
                phase,
                known,
            } => ReportOutcome::LimitReached {
                limit: *limit,
                phase: phase.clone(),
                known: known.clone(),
            },
        };
        Report {
            variables: ring.names.clone(),
            outcome,
            stats: self.stats.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::num::{Int, Rat};
    use crate::ring::{PrimeField, Rationals};

    fn eval<F: Ring>(ring: &PolyRing<F>, p: &Poly<F::Elem>, x: &[F::Elem]) -> F::Elem {
        let f = &ring.base;
        p.terms.iter().fold(f.zero(), |acc, (m, c)| {
            let t =
                m.0.iter()
                    .zip(x)
                    .fold(c.clone(), |t, (&e, v)| f.mul(&t, &f.pow(v, e as u32)));
            f.add(&acc, &t)
        })
    }

    fn check_points<F: Ring>(ring: &PolyRing<F>, eqs: &[Poly<F::Elem>], s: &Solved<F::Elem>) {
        if let Solution::Finite(fs) = &s.solution {
            for pt in &fs.points {
                for e in eqs {
                    assert!(ring.base.is_zero(&eval(ring, e, pt)), "{}", s.fmt(ring));
                }
            }
        }
    }

    fn finite<E: Clone + std::fmt::Debug>(s: &Solved<E>) -> &FiniteSolutions<E> {
        match &s.solution {
            Solution::Finite(fs) => fs,
            other => panic!("expected finitely many solutions, got {other:?}"),
        }
    }

    fn q(n: i64, d: i64) -> Rat {
        Rat::new(Int::from(n), Int::from(d))
    }

    /// The coefficients of `e·e − e` for `e = Σ c_k b_k`, where `b_i b_j = Σ_k mult[i][j][k] b_k`.
    fn idempotent_equations<F: Ring>(
        ring: &PolyRing<F>,
        mult: &[Vec<Vec<i64>>],
    ) -> Vec<Poly<F::Elem>> {
        let d = mult.len();
        (0..d)
            .map(|k| {
                let mut p = ring.neg(&ring.var(k));
                for (i, row) in mult.iter().enumerate() {
                    for (j, prod) in row.iter().enumerate() {
                        if prod[k] != 0 {
                            let t = ring.mul(&ring.var(i), &ring.var(j));
                            p = ring.add(&p, &ring.scale(&t, &ring.base.int(prod[k])));
                        }
                    }
                }
                p
            })
            .collect()
    }

    fn cyclic<F: Ring>(ring: &PolyRing<F>) -> Vec<Poly<F::Elem>> {
        let n = ring.nvars();
        let mut eqs = Vec::new();
        for k in 1..n {
            let mut p = ring.zero();
            for i in 0..n {
                let mut t = ring.one();
                for j in 0..k {
                    t = ring.mul(&t, &ring.var((i + j) % n));
                }
                p = ring.add(&p, &t);
            }
            eqs.push(p);
        }
        let mut t = ring.one();
        for i in 0..n {
            t = ring.mul(&t, &ring.var(i));
        }
        eqs.push(ring.sub(&t, &ring.one()));
        eqs
    }

    #[test]
    fn one_dimensional_algebra() {
        let r = PolyRing::new(Rationals, &["a"]);
        let eqs = idempotent_equations(&r, &[vec![vec![1]]]);
        let s = solve(&r, &eqs, &Limits::default()).unwrap();
        let fs = finite(&s);
        assert_eq!((fs.with_multiplicity, fs.distinct), (2, 2));
        assert!(fs.all_found);
        let mut vals: Vec<String> = fs.points.iter().map(|p| p[0].to_string()).collect();
        vals.sort();
        assert_eq!(vals, ["0", "1"]);
        check_points(&r, &eqs, &s);
    }

    #[test]
    fn product_algebra_has_four_idempotents() {
        let r = PolyRing::new(Rationals, &["a", "b"]);
        let mult = vec![vec![vec![1, 0], vec![0, 0]], vec![vec![0, 0], vec![0, 1]]];
        let eqs = idempotent_equations(&r, &mult);
        let s = solve(&r, &eqs, &Limits::default()).unwrap();
        let fs = finite(&s);
        assert_eq!(
            (fs.with_multiplicity, fs.distinct, fs.points.len()),
            (4, 4, 4)
        );
        assert!(fs.all_found);
        check_points(&r, &eqs, &s);
        let text = s.fmt(&r);
        assert!(text.contains("a = 1, b = 0"), "{text}");
    }

    #[test]
    fn matrix_idempotents_form_a_surface() {
        // e = [[a, b], [c, d]] in M_2(QQ); e·e = e.
        let r = PolyRing::new(Rationals, &["a", "b", "c", "d"]);
        let (a, b, c, d) = (r.var(0), r.var(1), r.var(2), r.var(3));
        let m = |x: &Poly<Rat>, y: &Poly<Rat>| r.mul(x, y);
        let eqs = vec![
            r.sub(&r.add(&m(&a, &a), &m(&b, &c)), &a),
            r.sub(&r.add(&m(&a, &b), &m(&b, &d)), &b),
            r.sub(&r.add(&m(&c, &a), &m(&d, &c)), &c),
            r.sub(&r.add(&m(&c, &b), &m(&d, &d)), &d),
        ];
        let s = solve(&r, &eqs, &Limits::default()).unwrap();
        match &s.solution {
            Solution::Family(fam) => {
                assert_eq!(fam.dimension, 2);
                assert!(!fam.basis.is_empty());
            }
            other => panic!("expected a family, got {other:?}"),
        }
        assert!(s.fmt(&r).contains("dimension 2"));
    }

    #[test]
    fn irrational_solutions_over_q() {
        let r = PolyRing::new(Rationals, &["x"]);
        let x = r.var(0);
        let eqs = vec![r.sub(&r.mul(&x, &x), &r.int(2))];
        let s = solve(&r, &eqs, &Limits::default()).unwrap();
        let fs = finite(&s);
        assert_eq!((fs.with_multiplicity, fs.distinct), (2, 2));
        assert!(fs.points.is_empty() && !fs.all_found);
        let text = s.fmt(&r);
        assert!(text.contains("x^2 - 2"), "{text}");
    }

    #[test]
    fn mixed_rational_and_irrational() {
        // (x − 1/2)(x² − 2) = 0, y = x²: 3 solutions, one rational.
        let r = PolyRing::new(Rationals, &["x", "y"]);
        let (x, y) = (r.var(0), r.var(1));
        let f1 = r.mul(
            &r.sub(&x, &r.constant(q(1, 2))),
            &r.sub(&r.mul(&x, &x), &r.int(2)),
        );
        let eqs = vec![f1, r.sub(&y, &r.mul(&x, &x))];
        let s = solve(&r, &eqs, &Limits::default()).unwrap();
        let fs = finite(&s);
        assert_eq!((fs.with_multiplicity, fs.distinct), (3, 3));
        assert_eq!(fs.points, vec![vec![q(1, 2), q(1, 4)]]);
        assert!(!fs.all_found);
        check_points(&r, &eqs, &s);
    }

    #[test]
    fn rational_root_enumeration() {
        // (x − 3)(x + 5)(2x − 1)(7x − 12345)(x² + 1): degree 6, four rational roots.
        let r = PolyRing::new(Rationals, &["x"]);
        let x = r.var(0);
        let lin = |a: i64, b: i64| r.sub(&r.scale(&x, &r.base.int(a)), &r.int(b));
        let mut p = r.add(&r.mul(&x, &x), &r.one());
        for (a, b) in [(1, 3), (1, -5), (2, 1), (7, 12345)] {
            p = r.mul(&p, &lin(a, b));
        }
        let s = solve(&r, std::slice::from_ref(&p), &Limits::default()).unwrap();
        let fs = finite(&s);
        assert_eq!(fs.distinct, 6);
        let mut vals: Vec<String> = fs.points.iter().map(|p| p[0].to_string()).collect();
        vals.sort();
        assert_eq!(vals, ["-5", "1/2", "12345/7", "3"]);
    }

    #[test]
    fn multiplicity_and_radical() {
        // x² = 0, y² = y: 4 with multiplicity, 2 distinct, both rational.
        let r = PolyRing::new(Rationals, &["x", "y"]);
        let (x, y) = (r.var(0), r.var(1));
        let eqs = vec![r.mul(&x, &x), r.sub(&r.mul(&y, &y), &y)];
        let s = solve(&r, &eqs, &Limits::default()).unwrap();
        let fs = finite(&s);
        assert_eq!(
            (fs.with_multiplicity, fs.distinct, fs.points.len()),
            (4, 2, 2)
        );
        assert!(fs.all_found);
    }

    #[test]
    fn over_gf5() {
        // x² = 1, y² = x over GF(5): x = ±1, then y² = 1 or y² = 4: four solutions.
        let r = PolyRing::new(PrimeField::new(5).unwrap(), &["x", "y"]);
        let (x, y) = (r.var(0), r.var(1));
        let eqs = vec![r.sub(&r.mul(&x, &x), &r.one()), r.sub(&r.mul(&y, &y), &x)];
        let s = solve(&r, &eqs, &Limits::default()).unwrap();
        let fs = finite(&s);
        assert_eq!(
            (fs.with_multiplicity, fs.distinct, fs.points.len()),
            (4, 4, 4)
        );
        assert!(fs.all_found);
        check_points(&r, &eqs, &s);
        // x² = 2 has no root in GF(5).
        let r1 = PolyRing::new(PrimeField::new(5).unwrap(), &["x"]);
        let x = r1.var(0);
        let eq = r1.sub(&r1.mul(&x, &x), &r1.int(2));
        let s = solve(&r1, &[eq], &Limits::default()).unwrap();
        let fs = finite(&s);
        assert_eq!((fs.distinct, fs.points.len(), fs.all_found), (2, 0, false));
    }

    #[test]
    fn large_prime_and_inseparable_multiplicity() {
        // (x − 1)(x − 2)(x − 3)(x² + 1) over GF(1000003) (≡ 3 mod 4: x² + 1 is irreducible).
        let r = PolyRing::new(PrimeField::new(1_000_003).unwrap(), &["x"]);
        let x = r.var(0);
        let mut f = r.add(&r.mul(&x, &x), &r.one());
        for k in 1..=3 {
            f = r.mul(&f, &r.sub(&x, &r.int(k)));
        }
        let s = solve(&r, std::slice::from_ref(&f), &Limits::default()).unwrap();
        let fs = finite(&s);
        let mut vals: Vec<u64> = fs.points.iter().map(|p| p[0]).collect();
        vals.sort();
        assert_eq!((fs.distinct, vals), (5, vec![1, 2, 3]));
        // x^5 − 1 = (x − 1)^5 over GF(5): one distinct solution of multiplicity 5.
        let r5 = PolyRing::new(PrimeField::new(5).unwrap(), &["x"]);
        let x = r5.var(0);
        let x5 = r5.mul(&r5.mul(&x, &x), &r5.mul(&x, &r5.mul(&x, &x)));
        let s = solve(&r5, &[r5.sub(&x5, &r5.one())], &Limits::default()).unwrap();
        let fs = finite(&s);
        assert_eq!(
            (fs.with_multiplicity, fs.distinct, fs.points.len()),
            (5, 1, 1)
        );
        assert!(fs.all_found);
    }

    #[test]
    fn inconsistent_system() {
        let r = PolyRing::new(Rationals, &["x", "y"]);
        let (x, y) = (r.var(0), r.var(1));
        let eqs = vec![
            r.sub(&r.mul(&x, &y), &r.one()),
            r.mul(&x, &x),
            r.sub(&y, &x),
        ];
        let s = solve(&r, &eqs, &Limits::default()).unwrap();
        assert_eq!(s.solution, Solution::None);
        assert!(s.fmt(&r).starts_with("no solutions"));
    }

    #[test]
    fn trivial_systems() {
        let r = PolyRing::new(Rationals, &["x", "y"]);
        let s = solve(&r, &[], &Limits::default()).unwrap();
        match &s.solution {
            Solution::Family(fam) => assert_eq!((fam.dimension, fam.basis.len()), (2, 0)),
            other => panic!("{other:?}"),
        }
        let r0 = PolyRing::new(Rationals, &[]);
        let s = solve(&r0, &[r0.zero()], &Limits::default()).unwrap();
        let fs = finite(&s);
        assert_eq!((fs.with_multiplicity, fs.points.len()), (1, 1));
        let s = solve(&r0, &[r0.int(3)], &Limits::default()).unwrap();
        assert_eq!(s.solution, Solution::None);
    }

    #[test]
    fn preconditions() {
        let r = PolyRing::new(crate::ring::Integers, &["x"]);
        let e = solve(&r, &[r.var(0)], &Limits::default()).unwrap_err();
        assert_eq!(e.kind, crate::error::ErrorKind::Precondition);
        let mut r = PolyRing::new(Rationals, &["q"]);
        r.laurent[0] = true;
        assert!(solve(&r, &[r.var(0)], &Limits::default()).is_err());
        let r = PolyRing::skew(Rationals, &["x", "y"]);
        assert!(solve(&r, &[r.var(0)], &Limits::default()).is_err());
    }

    #[test]
    fn limits_are_reported() {
        let r = PolyRing::new(Rationals, &["a", "b", "c", "d"]);
        let eqs = cyclic(&r);
        let lim = |max_basis, max_degree, max_steps| Limits {
            max_basis,
            max_degree,
            max_steps,
        };
        for (limits, kind) in [
            (lim(1000, 100, 20), LimitKind::Steps),
            (lim(3, 100, 1_000_000), LimitKind::BasisSize),
            (lim(1000, 3, 1_000_000), LimitKind::Degree),
        ] {
            let s = solve(&r, &eqs, &limits).unwrap();
            match &s.solution {
                Solution::LimitReached { limit, .. } => assert_eq!(*limit, kind),
                other => panic!("expected the {kind} limit, got {other:?}"),
            }
            assert!(s.fmt(&r).contains("limit reached"));
            // A step limit stops within one batch of steps (a basis update ticks |basis| + 1).
            assert!(s.stats.steps <= limits.max_steps + 100, "{:?}", s.stats);
        }
        let e = groebner_basis(&r, &eqs, TermOrder::Grevlex, &lim(1000, 100, 20)).unwrap_err();
        assert_eq!(e.kind, crate::error::ErrorKind::Limit);
    }

    #[test]
    fn root_search_respects_the_step_limit() {
        // x³ + x + N with N a product of two large primes: the divisor enumeration runs out.
        let r = PolyRing::new(Rationals, &["x"]);
        let x = r.var(0);
        let x3 = r.mul(&x, &r.mul(&x, &x));
        let big = r.constant(Rat::from_int(Int::from(1_000_000_007i64 * 998_244_353)));
        let p = r.add(&r.add(&x3, &x), &big);
        let limits = Limits {
            max_steps: 10_000,
            ..Limits::default()
        };
        let s = solve(&r, &[p], &limits).unwrap();
        match &s.solution {
            Solution::LimitReached {
                limit,
                phase,
                known,
            } => {
                assert_eq!(*limit, LimitKind::Steps);
                assert_eq!(phase, "solutions in the field");
                assert!(known.as_ref().unwrap().contains("3 distinct"));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn lex_basis() {
        // x² + y² − 1, x − y in lex: y² − 1/2, x − y.
        let r = PolyRing::new(Rationals, &["x", "y"]);
        let (x, y) = (r.var(0), r.var(1));
        let eqs = vec![
            r.sub(&r.add(&r.mul(&x, &x), &r.mul(&y, &y)), &r.one()),
            r.sub(&x, &y),
        ];
        let b = groebner_basis(&r, &eqs, TermOrder::Lex, &Limits::default()).unwrap();
        let text: Vec<String> = b.polys.iter().map(|p| r.fmt_elem(p)).collect();
        assert_eq!(text, ["y^2 - 1/2", "x - y"]);
    }

    #[test]
    fn cyclic4() {
        let r = PolyRing::new(Rationals, &["a", "b", "c", "d"]);
        let s = solve(&r, &cyclic(&r), &Limits::default()).unwrap();
        match &s.solution {
            Solution::Family(fam) => assert_eq!(fam.dimension, 1),
            other => panic!("{other:?}"),
        }
        assert!(s.stats.steps < 10_000, "{:?}", s.stats);
    }

    #[test]
    fn cyclic5_mod_p() {
        // 70 solutions over the algebraic closure; GF(11) contains the fifth roots of unity.
        let r = PolyRing::new(PrimeField::new(11).unwrap(), &["a", "b", "c", "d", "e"]);
        let eqs = cyclic(&r);
        let s = solve(&r, &eqs, &Limits::default()).unwrap();
        let fs = finite(&s);
        assert_eq!(fs.with_multiplicity, 70);
        assert!(fs.points.len() <= fs.distinct);
        check_points(&r, &eqs, &s);
    }

    /// Timings and printed answers of the benchmark-like systems:
    /// `cargo test --release -p diagcalc-core groebner::tests::timings -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn timings() {
        fn run<F: Ring>(name: &str, r: &PolyRing<F>, eqs: &[Poly<F::Elem>]) {
            let t = std::time::Instant::now();
            let s = solve(r, eqs, &Limits::default()).unwrap();
            let dt = t.elapsed();
            let text = s.fmt(r);
            let head: Vec<&str> = text.lines().take(6).collect();
            println!(
                "{name}: {dt:?}\n{}\n  ...{}\n",
                head.join("\n"),
                text.lines().last().unwrap()
            );
        }
        let r = PolyRing::new(Rationals, &["a", "b", "c", "d"]);
        let (a, b, c, d) = (r.var(0), r.var(1), r.var(2), r.var(3));
        let m = |x: &Poly<Rat>, y: &Poly<Rat>| r.mul(x, y);
        let m2 = vec![
            r.sub(&r.add(&m(&a, &a), &m(&b, &c)), &a),
            r.sub(&r.add(&m(&a, &b), &m(&b, &d)), &b),
            r.sub(&r.add(&m(&c, &a), &m(&d, &c)), &c),
            r.sub(&r.add(&m(&c, &b), &m(&d, &d)), &d),
        ];
        run("M_2(QQ) idempotents", &r, &m2);
        run("cyclic-4 over QQ", &r, &cyclic(&r));
        let r5 = PolyRing::new(Rationals, &["a", "b", "c", "d", "e"]);
        run("cyclic-5 over QQ", &r5, &cyclic(&r5));
        let f = PrimeField::new(11).unwrap();
        let r5p = PolyRing::new(f, &["a", "b", "c", "d", "e"]);
        run("cyclic-5 over GF(11)", &r5p, &cyclic(&r5p));
        let f = PrimeField::new(32003).unwrap();
        let r5p = PolyRing::new(f, &["a", "b", "c", "d", "e"]);
        run("cyclic-5 over GF(32003)", &r5p, &cyclic(&r5p));
        let r6 = PolyRing::new(
            PrimeField::new(32003).unwrap(),
            &["a", "b", "c", "d", "e", "f"],
        );
        run("cyclic-6 over GF(32003)", &r6, &cyclic(&r6));
        let r6 = PolyRing::new(Rationals, &["a", "b", "c", "d", "e", "f"]);
        run("cyclic-6 over QQ", &r6, &cyclic(&r6));
    }

    #[test]
    fn report_serializes() {
        let r = PolyRing::new(Rationals, &["a"]);
        let eqs = idempotent_equations(&r, &[vec![vec![1]]]);
        let s = solve(&r, &eqs, &Limits::default()).unwrap();
        let json = serde_json::to_value(s.report(&r)).unwrap();
        assert_eq!(json["kind"], "finite");
        assert_eq!(json["solutions_with_multiplicity"], 2);
        assert_eq!(json["variables"][0], "a");
        assert_eq!(json["basis"][0], "a^2 - a");
        assert!(json["stats"]["steps"].as_u64().unwrap() > 0);
    }
}
