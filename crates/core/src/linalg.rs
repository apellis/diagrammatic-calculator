//! Exact linear algebra over the ground rings.
//!
//! Matrices are dense and row-major ([`Mat`]); arithmetic goes through a ring context, as
//! everywhere in the core. What is offered depends on the ring:
//!
//! - **Fields** (`Ring::is_field`): the reduced row echelon form ([`rref`], and
//!   [`rref_with_transform`] for an invertible `T` with `T·A = rref(A)`), [`rank`], [`kernel`],
//!   [`row_space`], [`solve`], [`inverse`], and the incremental [`Reducer`], which keeps a reduced
//!   echelon basis of a growing span and reduces vectors modulo it (quotient coordinates).
//! - **Integral domains that are not fields** (ℤ, ℤ[√−1], ℤ[q], ℚ[q]): [`rank`] over the
//!   fraction field, by fraction-free (Bareiss) elimination ([`rank_bareiss`]). Laurent
//!   polynomial rings go through [`rank_poly`], which first multiplies each row by a monomial so
//!   that no negative powers remain (the rank does not change, and exact division then works).
//! - **ℤ**: the row Hermite normal form ([`hermite`], [`hermite_with_transform`]), the Smith
//!   invariant factors ([`smith_invariants`]), the abelian group ℤⁿ / (row span)
//!   ([`quotient_group`]) and a ℤ-basis of the integer kernel ([`kernel_z`]), so that ranks and
//!   torsion over ℤ are exact.
//! - **ℚ**: multimodular elimination with rational reconstruction, verified exactly over ℚ
//!   ([`rref_q`], [`rank_q`], [`kernel_q`]). **𝔽_p**: word-size elimination in Montgomery form
//!   ([`rref_mod_p`], [`rank_mod_p`]).
//!
//! # Fast paths behind the generic functions
//!
//! The generic entry points ([`rank`], [`rref`], [`kernel`], [`row_space`], [`solve`],
//! [`inverse`], [`rref_with_transform`], [`Reducer::from_matrix`]) take any `R: Ring` and need no
//! `'static` bound, so code generic over the ground ring calls them directly. They choose a fast
//! path by looking at the *matrix*, not the ring type: when the matrix is not tiny and every
//! entry lies in the prime field (ℚ in characteristic 0, 𝔽_p in odd characteristic p), the
//! computation runs multimodularly over ℚ or on machine words over 𝔽_p, and the answer is mapped
//! back with `Ring::rational` or `Ring::int`. Membership is checked exactly, entry by entry: the
//! entry is printed, read back as `n/d` (or `k`), and its image `Ring::rational(n, d)` (or
//! `Ring::int(k)`) must equal the entry. Ranks, reduced echelon forms and kernels do not change
//! under a field extension, so the fast path is exact; it covers ℚ itself, ℤ (rank over ℚ), 𝔽_p,
//! and matrices with constant entries over ℚ(q) or ℚ[q]. Otherwise the generic algorithm runs in
//! the ring. Callers holding a `Mat<Rat>` can call [`rank_q`], [`rref_q`], [`kernel_q`] directly
//! and skip the membership check.
//!
//! Over ℚ, a rank modulo a prime is a lower bound for the rank over ℚ. A multimodular reduced
//! echelon form is accepted only after an exact check over ℚ that the kernel basis it gives (one
//! vector per free column) is annihilated by the matrix: that kernel has dimension `cols − r_p`,
//! so the rank is at most `r_p`, hence exactly `r_p`, and the echelon form is the true one.

use crate::error::{Error, Result};
use crate::num::{Int, Rat};
use crate::poly::{Monomial, Poly, PolyRing};
use crate::ring::{Integers, PrimeField, Rationals, Ring, is_prime_u64};
use num_bigint::BigInt;
use num_integer::Integer;
use num_traits::{One, Signed, ToPrimitive, Zero};
use std::cmp::Ordering;
use std::fmt;

// ---------------------------------------------------------------------------------------------
// Matrices
// ---------------------------------------------------------------------------------------------

/// A dense matrix with entries in some ring, stored row by row.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Mat<E> {
    rows: usize,
    cols: usize,
    data: Vec<E>,
}

impl<E: Clone> Mat<E> {
    /// A `rows × cols` matrix from its entries, row by row.
    pub fn new(rows: usize, cols: usize, data: Vec<E>) -> Result<Self> {
        if data.len() != rows * cols {
            return Err(Error::invalid(format!(
                "a {rows} × {cols} matrix needs {} entries, not {}",
                rows * cols,
                data.len()
            )));
        }
        Ok(Mat { rows, cols, data })
    }

    /// The zero matrix.
    pub fn zeros<R: Ring<Elem = E>>(ring: &R, rows: usize, cols: usize) -> Self {
        Mat {
            rows,
            cols,
            data: vec![ring.zero(); rows * cols],
        }
    }

    /// The `n × n` identity matrix.
    pub fn identity<R: Ring<Elem = E>>(ring: &R, n: usize) -> Self {
        let mut m = Self::zeros(ring, n, n);
        for i in 0..n {
            m.data[i * n + i] = ring.one();
        }
        m
    }

    /// The matrix with entry `f(i, j)` in row `i`, column `j`.
    pub fn from_fn(rows: usize, cols: usize, mut f: impl FnMut(usize, usize) -> E) -> Self {
        let mut data = Vec::with_capacity(rows * cols);
        for i in 0..rows {
            for j in 0..cols {
                data.push(f(i, j));
            }
        }
        Mat { rows, cols, data }
    }

    /// A matrix from its rows, each of length `cols` (given separately so that a matrix with no
    /// rows still has a width).
    pub fn from_rows(cols: usize, rows: Vec<Vec<E>>) -> Result<Self> {
        let n = rows.len();
        let mut data = Vec::with_capacity(n * cols);
        for (i, r) in rows.into_iter().enumerate() {
            if r.len() != cols {
                return Err(Error::invalid(format!(
                    "row {} has {} entries, but the matrix has {cols} columns",
                    i + 1,
                    r.len()
                )));
            }
            data.extend(r);
        }
        Ok(Mat {
            rows: n,
            cols,
            data,
        })
    }

    /// A matrix of integers, mapped into the ring; the width is that of the first row.
    pub fn from_ints<R: Ring<Elem = E>>(ring: &R, rows: &[&[i64]]) -> Result<Self> {
        let cols = rows.first().map_or(0, |r| r.len());
        let rows = rows
            .iter()
            .map(|r| r.iter().map(|&x| ring.int(x)).collect())
            .collect();
        Self::from_rows(cols, rows)
    }

    pub fn nrows(&self) -> usize {
        self.rows
    }

    pub fn ncols(&self) -> usize {
        self.cols
    }

    /// The entry in row `i`, column `j` (0-based); panics outside the matrix.
    pub fn get(&self, i: usize, j: usize) -> &E {
        assert!(
            i < self.rows && j < self.cols,
            "entry ({i}, {j}) is outside the matrix"
        );
        &self.data[i * self.cols + j]
    }

    /// Replaces the entry in row `i`, column `j`; panics outside the matrix.
    pub fn set(&mut self, i: usize, j: usize, v: E) {
        assert!(
            i < self.rows && j < self.cols,
            "entry ({i}, {j}) is outside the matrix"
        );
        self.data[i * self.cols + j] = v;
    }

    /// Row `i` as a slice.
    pub fn row(&self, i: usize) -> &[E] {
        &self.data[i * self.cols..(i + 1) * self.cols]
    }

    /// All entries, row by row.
    pub fn data(&self) -> &[E] {
        &self.data
    }

    /// The rows as vectors.
    pub fn to_rows(&self) -> Vec<Vec<E>> {
        (0..self.rows).map(|i| self.row(i).to_vec()).collect()
    }

    pub fn transpose(&self) -> Self {
        Mat::from_fn(self.cols, self.rows, |i, j| {
            self.data[j * self.cols + i].clone()
        })
    }

    /// Applies `f` to every entry (for example to change the ring).
    pub fn map<F>(&self, f: impl FnMut(&E) -> F) -> Mat<F> {
        Mat {
            rows: self.rows,
            cols: self.cols,
            data: self.data.iter().map(f).collect(),
        }
    }

    /// The block matrix `[self | other]`.
    pub fn hstack(&self, other: &Mat<E>) -> Result<Self> {
        if self.rows != other.rows {
            return Err(Error::invalid(format!(
                "cannot put a matrix with {} rows beside one with {} rows",
                other.rows, self.rows
            )));
        }
        let cols = self.cols + other.cols;
        let mut data = Vec::with_capacity(self.rows * cols);
        for i in 0..self.rows {
            data.extend_from_slice(self.row(i));
            data.extend_from_slice(other.row(i));
        }
        Ok(Mat {
            rows: self.rows,
            cols,
            data,
        })
    }

    /// The columns `lo..hi` as a matrix.
    fn columns(&self, lo: usize, hi: usize) -> Self {
        Mat::from_fn(self.rows, hi - lo, |i, j| {
            self.data[i * self.cols + lo + j].clone()
        })
    }

    /// The product `self · other`.
    pub fn mul<R: Ring<Elem = E>>(&self, ring: &R, other: &Mat<E>) -> Result<Self> {
        if self.cols != other.rows {
            return Err(Error::invalid(format!(
                "cannot multiply a {} × {} matrix by a {} × {} matrix",
                self.rows, self.cols, other.rows, other.cols
            )));
        }
        let n = other.cols;
        let mut out = Mat::zeros(ring, self.rows, n);
        for i in 0..self.rows {
            for (k, a) in self.row(i).iter().enumerate() {
                if ring.is_zero(a) {
                    continue;
                }
                for (x, b) in out.data[i * n..(i + 1) * n].iter_mut().zip(other.row(k)) {
                    if !ring.is_zero(b) {
                        *x = ring.add(x, &ring.mul(a, b));
                    }
                }
            }
        }
        Ok(out)
    }

    /// The product `self · v` with a column vector.
    pub fn mul_vec<R: Ring<Elem = E>>(&self, ring: &R, v: &[E]) -> Result<Vec<E>> {
        if self.cols != v.len() {
            return Err(Error::invalid(format!(
                "cannot multiply a {} × {} matrix by a vector of length {}",
                self.rows,
                self.cols,
                v.len()
            )));
        }
        Ok((0..self.rows)
            .map(|i| {
                let mut acc = ring.zero();
                for (a, b) in self.row(i).iter().zip(v) {
                    if !ring.is_zero(a) && !ring.is_zero(b) {
                        acc = ring.add(&acc, &ring.mul(a, b));
                    }
                }
                acc
            })
            .collect())
    }
}

fn swap_rows<E>(d: &mut [E], n: usize, i: usize, j: usize) {
    if i != j {
        let (lo, hi) = (i.min(j), i.max(j));
        let (a, b) = d.split_at_mut(hi * n);
        a[lo * n..(lo + 1) * n].swap_with_slice(&mut b[..n]);
    }
}

/// The type name of a ring without module paths, for error messages: `PolyRing<Rationals>`.
fn ring_name<R>() -> String {
    let full = std::any::type_name::<R>();
    let mut out = String::new();
    let mut seg = String::new();
    let flush = |seg: &mut String, out: &mut String| {
        out.push_str(seg.rsplit("::").next().unwrap_or(""));
        seg.clear();
    };
    for ch in full.chars() {
        if ch.is_alphanumeric() || ch == '_' || ch == ':' {
            seg.push(ch);
        } else {
            flush(&mut seg, &mut out);
            out.push(ch);
        }
    }
    flush(&mut seg, &mut out);
    out
}

fn require_field<R: Ring>(ring: &R, what: &str) -> Result<()> {
    if ring.is_field() {
        Ok(())
    } else {
        Err(Error::precondition(format!(
            "{what} needs a field, and {} is not one",
            ring_name::<R>()
        )))
    }
}

fn invert<R: Ring>(ring: &R, a: &R::Elem) -> Result<R::Elem> {
    ring.inv(a).ok_or_else(|| {
        Error::precondition(format!(
            "the nonzero element {} has no inverse in {}, which claims to be a field",
            ring.fmt_elem(a),
            ring_name::<R>()
        ))
    })
}

// ---------------------------------------------------------------------------------------------
// Echelon forms over a field
// ---------------------------------------------------------------------------------------------

/// A reduced row echelon form: pivot entries 1, each pivot column zero outside its pivot row,
/// zero rows last.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Echelon<E> {
    /// The reduced row echelon form, of the same size as the input.
    pub rref: Mat<E>,
    /// The pivot columns, increasing; row `i` of `rref` has its pivot in column `pivots[i]`.
    pub pivots: Vec<usize>,
    /// An invertible `T` with `T · A = rref`, when requested.
    pub transform: Option<Mat<E>>,
}

impl<E: Clone> Echelon<E> {
    pub fn rank(&self) -> usize {
        self.pivots.len()
    }

    /// The columns that are not pivots, increasing.
    pub fn free_columns(&self) -> Vec<usize> {
        free_columns(&self.pivots, self.rref.cols)
    }

    /// The standard basis of the right kernel `{x : A x = 0}`: one vector per free column `f`,
    /// with 1 in column `f`, 0 in the other free columns and `-rref[i][f]` in the pivot column of
    /// row `i`.
    pub fn kernel<R: Ring<Elem = E>>(&self, ring: &R) -> Vec<Vec<E>> {
        kernel_from_rref(ring, &self.rref, &self.pivots, self.rref.cols)
    }

    /// The nonzero rows of the echelon form: a basis of the row space.
    pub fn row_space(&self) -> Vec<Vec<E>> {
        (0..self.rank())
            .map(|i| self.rref.row(i).to_vec())
            .collect()
    }
}

fn free_columns(pivots: &[usize], n: usize) -> Vec<usize> {
    let mut is_pivot = vec![false; n];
    for &p in pivots {
        if p < n {
            is_pivot[p] = true;
        }
    }
    (0..n).filter(|&j| !is_pivot[j]).collect()
}

/// Kernel basis of the first `n` columns of a reduced echelon form whose pivots in those
/// columns are `pivots` (pivots at `n` or beyond are ignored).
fn kernel_from_rref<R: Ring>(
    ring: &R,
    rref: &Mat<R::Elem>,
    pivots: &[usize],
    n: usize,
) -> Vec<Vec<R::Elem>> {
    let piv: Vec<usize> = pivots.iter().copied().filter(|&p| p < n).collect();
    free_columns(&piv, n)
        .into_iter()
        .map(|f| {
            let mut v = vec![ring.zero(); n];
            v[f] = ring.one();
            for (i, &p) in piv.iter().enumerate() {
                v[p] = ring.neg(rref.get(i, f));
            }
            v
        })
        .collect()
}

/// Reduced row echelon form in the ring itself, by fraction-free Gauss–Jordan elimination: at a
/// pivot `p` every other row becomes `(p · row − f · pivot row) / prev`, with `prev` the previous
/// pivot, and the rows are divided by their pivots only at the end. Every step is an invertible
/// row operation, so the result is right in any field; when the entries are polynomials (say over
/// ℚ(q)) the intermediate entries are minors, the divisions by `prev` are exact, and no
/// rational-function arithmetic with unrelated denominators happens until the final division,
/// which is far cheaper than plain Gauss–Jordan there.
fn rref_generic<R: Ring>(ring: &R, a: &Mat<R::Elem>) -> Result<Echelon<R::Elem>> {
    let (m, n) = (a.rows, a.cols);
    let mut d = a.data.clone();
    let mut pivots = Vec::new();
    let mut prev = ring.one();
    let mut r = 0;
    for c in 0..n {
        if r == m {
            break;
        }
        let Some(i) = (r..m).find(|&i| !ring.is_zero(&d[i * n + c])) else {
            continue;
        };
        swap_rows(&mut d, n, i, r);
        let piv = d[r * n + c].clone();
        let prev_inv = invert(ring, &prev)?;
        let prev_is_one = ring.is_one(&prev);
        let prow: Vec<R::Elem> = d[r * n..(r + 1) * n].to_vec();
        for i in (0..m).filter(|&i| i != r) {
            let f = d[i * n + c].clone();
            if ring.is_zero(&f) && ring.is_one(&piv) && prev_is_one {
                continue;
            }
            for (x, y) in d[i * n..(i + 1) * n].iter_mut().zip(&prow) {
                if ring.is_zero(x) && (ring.is_zero(&f) || ring.is_zero(y)) {
                    continue;
                }
                let mut v = ring.mul(x, &piv);
                if !ring.is_zero(&f) && !ring.is_zero(y) {
                    v = ring.sub(&v, &ring.mul(&f, y));
                }
                *x = if prev_is_one || ring.is_zero(&v) {
                    v
                } else {
                    ring.div_exact(&v, &prev)
                        .unwrap_or_else(|| ring.mul(&v, &prev_inv))
                };
            }
            d[i * n + c] = ring.zero();
        }
        prev = piv;
        pivots.push(c);
        r += 1;
    }
    for (i, &p) in pivots.iter().enumerate() {
        let inv = invert(ring, &d[i * n + p])?;
        if ring.is_one(&inv) {
            continue;
        }
        for x in &mut d[i * n + p..(i + 1) * n] {
            if !ring.is_zero(x) {
                *x = ring.mul(x, &inv);
            }
        }
    }
    Ok(Echelon {
        rref: Mat {
            rows: m,
            cols: n,
            data: d,
        },
        pivots,
        transform: None,
    })
}

// ---------------------------------------------------------------------------------------------
// Fast-path detection: entries in the prime field
// ---------------------------------------------------------------------------------------------

/// Matrices with fewer entries are reduced directly in the ring.
const FAST_PATH_MIN_ENTRIES: usize = 64;

enum Lift {
    /// Every entry is the image of a rational number.
    Q(Mat<Rat>),
    /// Every entry is the image of an integer modulo the odd prime `p`; residues in `0..p`.
    Fp(u64, Vec<u64>),
}

fn parse_rational(s: &str) -> Option<(BigInt, BigInt)> {
    let (n, d) = s.split_once('/').unwrap_or((s, "1"));
    let n: BigInt = n.trim().parse().ok()?;
    let d: BigInt = d.trim().parse().ok()?;
    (!d.is_zero()).then_some((n, d))
}

fn lift_q<R: Ring>(ring: &R, a: &Mat<R::Elem>) -> Option<Mat<Rat>> {
    let mut data = Vec::with_capacity(a.data.len());
    for e in &a.data {
        if ring.is_zero(e) {
            data.push(Rat::ZERO);
            continue;
        }
        let (n, d) = parse_rational(&ring.fmt_elem(e))?;
        if ring.rational(&n, &d).as_ref() != Some(e) {
            return None;
        }
        data.push(Rat::new(Int::from(n), Int::from(d)));
    }
    Some(Mat {
        rows: a.rows,
        cols: a.cols,
        data,
    })
}

fn lift_fp<R: Ring>(ring: &R, a: &Mat<R::Elem>, p: u64) -> Option<Vec<u64>> {
    let mut data = Vec::with_capacity(a.data.len());
    for e in &a.data {
        if ring.is_zero(e) {
            data.push(0);
            continue;
        }
        let k: i128 = ring.fmt_elem(e).trim().parse().ok()?;
        let r = k.rem_euclid(p as i128) as u64;
        if ring.int(r as i64) != *e {
            return None;
        }
        data.push(r);
    }
    Some(data)
}

fn lift<R: Ring>(ring: &R, a: &Mat<R::Elem>) -> Option<Lift> {
    if a.data.len() < FAST_PATH_MIN_ENTRIES {
        return None;
    }
    match ring.characteristic() {
        0 => lift_q(ring, a).map(Lift::Q),
        p if p > 2 && PrimeField::new(p).is_some() => lift_fp(ring, a, p).map(|d| Lift::Fp(p, d)),
        _ => None,
    }
}

fn from_q<R: Ring>(ring: &R, x: &Rat) -> Result<R::Elem> {
    ring.rational(&x.numer().to_bigint(), &x.denom().to_bigint())
        .ok_or_else(|| {
            Error::precondition(format!(
                "the rational number {x} has no image in {}",
                ring_name::<R>()
            ))
        })
}

fn map_echelon<E: Clone, F>(
    e: Echelon<E>,
    mut f: impl FnMut(&E) -> Result<F>,
) -> Result<Echelon<F>> {
    let data = e.rref.data.iter().map(&mut f).collect::<Result<Vec<F>>>()?;
    let transform = match e.transform {
        Some(t) => Some(Mat {
            rows: t.rows,
            cols: t.cols,
            data: t.data.iter().map(&mut f).collect::<Result<Vec<F>>>()?,
        }),
        None => None,
    };
    Ok(Echelon {
        rref: Mat {
            rows: e.rref.rows,
            cols: e.rref.cols,
            data,
        },
        pivots: e.pivots,
        transform,
    })
}

// ---------------------------------------------------------------------------------------------
// Generic entry points
// ---------------------------------------------------------------------------------------------

/// The reduced row echelon form over a field. See the module documentation for the fast paths
/// over ℚ and 𝔽_p.
pub fn rref<R: Ring>(ring: &R, a: &Mat<R::Elem>) -> Result<Echelon<R::Elem>> {
    require_field(ring, "row reduction")?;
    match lift(ring, a) {
        Some(Lift::Q(q)) => map_echelon(rref_q(&q), |x| from_q(ring, x)),
        Some(Lift::Fp(p, d)) => {
            let (d, pivots) = fp_eliminate(p, d, a.rows, a.cols, true);
            Ok(Echelon {
                rref: Mat {
                    rows: a.rows,
                    cols: a.cols,
                    data: d.into_iter().map(|v| ring.int(v as i64)).collect(),
                },
                pivots,
                transform: None,
            })
        }
        None => rref_generic(ring, a),
    }
}

/// The reduced row echelon form together with an invertible `T` (`rows × rows`) such that
/// `T · A = rref(A)`, read off from the reduced echelon form of `[A | I]`.
pub fn rref_with_transform<R: Ring>(ring: &R, a: &Mat<R::Elem>) -> Result<Echelon<R::Elem>> {
    let n = a.cols;
    let aug = a.hstack(&Mat::identity(ring, a.rows))?;
    let e = rref(ring, &aug)?;
    // The left block of rref([A | I]) is rref(A): its nonzero rows come first.
    Ok(Echelon {
        rref: e.rref.columns(0, n),
        pivots: e.pivots.iter().copied().filter(|&p| p < n).collect(),
        transform: Some(e.rref.columns(n, n + a.rows)),
    })
}

/// The rank: over a field, or over the fraction field of an integral domain (by fraction-free
/// elimination). Laurent polynomial rings need [`rank_poly`].
pub fn rank<R: Ring>(ring: &R, a: &Mat<R::Elem>) -> Result<usize> {
    if !ring.is_field() && !ring.is_domain() {
        return Err(Error::precondition(format!(
            "rank needs a field or an integral domain, and {} is neither",
            ring_name::<R>()
        )));
    }
    if a.rows == 0 || a.cols == 0 {
        return Ok(0);
    }
    match lift(ring, a) {
        Some(Lift::Q(q)) => Ok(rank_q(&q)),
        Some(Lift::Fp(p, d)) => Ok(fp_eliminate(p, d, a.rows, a.cols, false).1.len()),
        None => rank_bareiss(ring, a),
    }
}

/// A basis of the right kernel `{x : A x = 0}` over a field, in the standard form of
/// [`Echelon::kernel`].
pub fn kernel<R: Ring>(ring: &R, a: &Mat<R::Elem>) -> Result<Vec<Vec<R::Elem>>> {
    Ok(rref(ring, a)?.kernel(ring))
}

/// A basis of the row space over a field: the nonzero rows of the reduced echelon form.
pub fn row_space<R: Ring>(ring: &R, a: &Mat<R::Elem>) -> Result<Vec<Vec<R::Elem>>> {
    Ok(rref(ring, a)?.row_space())
}

/// A particular solution of a linear system and a basis of the kernel of its matrix.
pub type Solution<E> = (Vec<E>, Vec<Vec<E>>);

/// A solution of `A x = b`, as a particular solution and a basis of the kernel of `A` (the
/// solutions are the particular one plus the span of the kernel), or `None` if there is none.
/// Over a field.
pub fn solve<R: Ring>(
    ring: &R,
    a: &Mat<R::Elem>,
    b: &[R::Elem],
) -> Result<Option<Solution<R::Elem>>> {
    if b.len() != a.rows {
        return Err(Error::invalid(format!(
            "the right-hand side has length {}, but the matrix has {} rows",
            b.len(),
            a.rows
        )));
    }
    let n = a.cols;
    let col = Mat {
        rows: a.rows,
        cols: 1,
        data: b.to_vec(),
    };
    let e = rref(ring, &a.hstack(&col)?)?;
    if e.pivots.last() == Some(&n) {
        return Ok(None);
    }
    let mut x = vec![ring.zero(); n];
    for (i, &p) in e.pivots.iter().enumerate() {
        x[p] = e.rref.get(i, n).clone();
    }
    let ker = kernel_from_rref(ring, &e.rref, &e.pivots, n);
    Ok(Some((x, ker)))
}

/// The inverse of a square matrix over a field, or `None` if it is singular.
pub fn inverse<R: Ring>(ring: &R, a: &Mat<R::Elem>) -> Result<Option<Mat<R::Elem>>> {
    if a.rows != a.cols {
        return Err(Error::invalid(format!(
            "only a square matrix has an inverse, not a {} × {} one",
            a.rows, a.cols
        )));
    }
    let n = a.rows;
    let e = rref(ring, &a.hstack(&Mat::identity(ring, n))?)?;
    if e.pivots.iter().filter(|&&p| p < n).count() < n {
        return Ok(None);
    }
    Ok(Some(e.rref.columns(n, 2 * n)))
}

// ---------------------------------------------------------------------------------------------
// The incremental reducer
// ---------------------------------------------------------------------------------------------

/// A span of row vectors over a field, kept as a fully reduced echelon basis (pivot entries 1,
/// each pivot column zero in every other basis vector), with reduction of any vector modulo the
/// span.
///
/// Reducing `v` subtracts `v[p] · b_p` for each basis vector `b_p` with pivot `p`; the result is
/// the unique representative of `v + span` that vanishes in every pivot column. Its entries in
/// the free columns are the coordinates of `v` in the quotient space with respect to the images
/// of the standard basis vectors of the free columns. When a vector is inserted, its pivot is the
/// lowest-index nonzero column of its reduction, so callers who want certain columns to survive
/// as quotient-basis representatives order them last.
#[derive(Clone, Debug)]
pub struct Reducer<R: Ring> {
    ring: R,
    ncols: usize,
    /// Basis vectors, sorted by pivot.
    rows: Vec<Vec<R::Elem>>,
    /// Pivot columns, increasing.
    pivots: Vec<usize>,
}

impl<R: Ring> Reducer<R> {
    /// The zero span in a space of `ncols`-component vectors. The ring must be a field.
    pub fn new(ring: R, ncols: usize) -> Self {
        debug_assert!(ring.is_field(), "a span reducer needs a field");
        Reducer {
            ring,
            ncols,
            rows: Vec::new(),
            pivots: Vec::new(),
        }
    }

    /// The span of the rows of a matrix (computed with [`rref`], so the fast paths apply).
    pub fn from_matrix(ring: R, a: &Mat<R::Elem>) -> Result<Self> {
        let e = rref(&ring, a)?;
        Ok(Reducer {
            ncols: a.cols,
            rows: e.row_space(),
            pivots: e.pivots,
            ring,
        })
    }

    pub fn ring(&self) -> &R {
        &self.ring
    }

    pub fn ncols(&self) -> usize {
        self.ncols
    }

    /// The dimension of the span.
    pub fn rank(&self) -> usize {
        self.pivots.len()
    }

    /// Pivot columns, sorted increasingly.
    pub fn pivots(&self) -> Vec<usize> {
        self.pivots.clone()
    }

    /// Columns that are not pivots, sorted: their standard basis vectors give a basis of the
    /// quotient space.
    pub fn free_columns(&self) -> Vec<usize> {
        free_columns(&self.pivots, self.ncols)
    }

    /// The reduced echelon basis, sorted by pivot.
    pub fn basis(&self) -> &[Vec<R::Elem>] {
        &self.rows
    }

    /// The vector reduced modulo the span: zero in every pivot column. Panics if the length is
    /// not `ncols`.
    pub fn reduce(&self, v: &[R::Elem]) -> Vec<R::Elem> {
        assert_eq!(
            v.len(),
            self.ncols,
            "vector length differs from the reducer's"
        );
        let ring = &self.ring;
        let mut w = v.to_vec();
        for (row, &p) in self.rows.iter().zip(&self.pivots) {
            if ring.is_zero(&w[p]) {
                continue;
            }
            let f = w[p].clone();
            for (x, y) in w[p..].iter_mut().zip(&row[p..]) {
                if !ring.is_zero(y) {
                    *x = ring.sub(x, &ring.mul(&f, y));
                }
            }
        }
        w
    }

    /// True if the vector lies in the span.
    pub fn contains(&self, v: &[R::Elem]) -> bool {
        self.reduce(v).iter().all(|x| self.ring.is_zero(x))
    }

    /// The coordinates of `v` in the quotient space: its reduction read at the free columns, in
    /// the order of [`Reducer::free_columns`].
    pub fn quotient_coords(&self, v: &[R::Elem]) -> Vec<R::Elem> {
        let w = self.reduce(v);
        self.free_columns()
            .into_iter()
            .map(|j| w[j].clone())
            .collect()
    }

    /// Adds a vector to the span; returns true if the rank grew. Panics if the length is not
    /// `ncols`.
    pub fn insert(&mut self, v: Vec<R::Elem>) -> bool {
        let mut w = self.reduce(&v);
        let ring = &self.ring;
        let Some(c) = w.iter().position(|x| !ring.is_zero(x)) else {
            return false;
        };
        let inv = ring
            .inv(&w[c])
            .expect("a nonzero element of a field is invertible");
        for x in &mut w[c..] {
            if !ring.is_zero(x) {
                *x = ring.mul(x, &inv);
            }
        }
        // Clear the new pivot column from the other basis vectors.
        for row in &mut self.rows {
            if ring.is_zero(&row[c]) {
                continue;
            }
            let f = row[c].clone();
            for (x, y) in row[c..].iter_mut().zip(&w[c..]) {
                if !ring.is_zero(y) {
                    *x = ring.sub(x, &ring.mul(&f, y));
                }
            }
        }
        let at = self.pivots.partition_point(|&p| p < c);
        self.pivots.insert(at, c);
        self.rows.insert(at, w);
        true
    }
}

// ---------------------------------------------------------------------------------------------
// Integral domains: fraction-free elimination
// ---------------------------------------------------------------------------------------------

/// The rank over the fraction field of an integral domain, by fraction-free (Bareiss)
/// elimination: after `k` pivots every remaining entry is a `(k+1) × (k+1)` minor, and the
/// division by the previous pivot is exact. Fails with a precondition error if the ring's
/// `div_exact` cannot perform that division (for example a Laurent polynomial ring, which only
/// divides by units: use [`rank_poly`]).
pub fn rank_bareiss<R: Ring>(ring: &R, a: &Mat<R::Elem>) -> Result<usize> {
    if !ring.is_domain() {
        return Err(Error::precondition(format!(
            "fraction-free elimination needs an integral domain, and {} is not one",
            ring_name::<R>()
        )));
    }
    let (m, n) = (a.rows, a.cols);
    let mut d = a.data.clone();
    let mut prev = ring.one();
    let mut r = 0;
    for c in 0..n {
        if r == m {
            break;
        }
        let Some(i) = (r..m).find(|&i| !ring.is_zero(&d[i * n + c])) else {
            continue;
        };
        swap_rows(&mut d, n, i, r);
        let piv = d[r * n + c].clone();
        let prev_is_one = ring.is_one(&prev);
        for i in r + 1..m {
            let f = d[i * n + c].clone();
            for j in c + 1..n {
                let mut x = ring.mul(&d[i * n + j], &piv);
                if !ring.is_zero(&f) && !ring.is_zero(&d[r * n + j]) {
                    x = ring.sub(&x, &ring.mul(&f, &d[r * n + j]));
                }
                d[i * n + j] = if prev_is_one || ring.is_zero(&x) {
                    x
                } else {
                    ring.div_exact(&x, &prev).ok_or_else(|| {
                        Error::precondition(format!(
                            "fraction-free elimination needs exact division by {}, which {} \
                             cannot do (for a Laurent polynomial ring use rank_poly)",
                            ring.fmt_elem(&prev),
                            ring_name::<R>()
                        ))
                    })?
                };
            }
            d[i * n + c] = ring.zero();
        }
        prev = piv;
        r += 1;
    }
    Ok(r)
}

/// The rank over the fraction field of a commutative polynomial or Laurent polynomial ring.
///
/// Each row is first multiplied by a monomial (a unit of the Laurent ring) so that no negative
/// exponent remains; this does not change the rank, and the entries then lie in the polynomial
/// subring, where exact division works and [`rank`] applies.
pub fn rank_poly<R: Ring>(ring: &PolyRing<R>, a: &Mat<Poly<R::Elem>>) -> Result<usize> {
    if !ring.is_domain() {
        return Err(Error::precondition(
            "rank needs commuting variables; this polynomial ring has anticommuting ones",
        ));
    }
    if !ring.laurent.iter().any(|&l| l) {
        return rank(ring, a);
    }
    let nv = ring.nvars();
    let plain = PolyRing {
        laurent: vec![false; nv],
        ..ring.clone()
    };
    let mut data = Vec::with_capacity(a.data.len());
    for i in 0..a.rows {
        let row = a.row(i);
        let mut shift = vec![0i32; nv];
        for p in row {
            for (mono, _) in &p.terms {
                for (s, &e) in shift.iter_mut().zip(mono.0.iter()) {
                    *s = (*s).min(e);
                }
            }
        }
        for p in row {
            let terms = p
                .terms
                .iter()
                .map(|(mono, c)| {
                    let m = mono.0.iter().zip(&shift).map(|(e, s)| e - s).collect();
                    (Monomial(m), c.clone())
                })
                .collect();
            data.push(plain.normalize(terms));
        }
    }
    rank(
        &plain,
        &Mat {
            rows: a.rows,
            cols: a.cols,
            data,
        },
    )
}

// ---------------------------------------------------------------------------------------------
// ℤ: Hermite and Smith normal forms
// ---------------------------------------------------------------------------------------------

/// `(g, s, t)` with `g = gcd(a, b) = s a + t b` and `g ≥ 0`.
fn xgcd(a: &Int, b: &Int) -> (Int, Int, Int) {
    let (mut r0, mut r1) = (a.clone(), b.clone());
    let (mut s0, mut s1) = (Int::ONE, Int::ZERO);
    let (mut t0, mut t1) = (Int::ZERO, Int::ONE);
    while !r1.is_zero() {
        let q = r0.div_rem(&r1).0;
        let r2 = r0.sub(&q.mul(&r1));
        let s2 = s0.sub(&q.mul(&s1));
        let t2 = t0.sub(&q.mul(&t1));
        (r0, r1, s0, s1, t0, t1) = (r1, r2, s1, s2, t1, t2);
    }
    if r0.is_negative() {
        (r0.neg(), s0.neg(), t0.neg())
    } else {
        (r0, s0, t0)
    }
}

fn lcm(a: &Int, b: &Int) -> Int {
    a.div_rem(&a.gcd(b)).0.mul(b).abs()
}

/// `x − q·y`, entrywise from column `from` on (earlier entries unchanged).
fn sub_multiple(x: &mut [Int], y: &[Int], q: &Int, from: usize) {
    if q.is_zero() {
        return;
    }
    for (a, b) in x[from..].iter_mut().zip(&y[from..]) {
        if !b.is_zero() {
            *a = a.sub(&q.mul(b));
        }
    }
}

/// The row Hermite normal form of an integer matrix.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hermite {
    /// The nonzero rows of the Hermite normal form `H`: echelon, pivots positive, entries above
    /// each pivot in `0..pivot`. Its rows are a ℤ-basis of the row lattice of `A`.
    pub h: Mat<Int>,
    /// The pivot columns, increasing.
    pub pivots: Vec<usize>,
    /// When requested, a unimodular `U` (`rows × rows`) with `U · A = [H; 0]`.
    pub transform: Option<Mat<Int>>,
}

/// The row Hermite normal form of an integer matrix (unique for its row lattice).
pub fn hermite(a: &Mat<Int>) -> Hermite {
    hermite_impl(a, false)
}

/// The row Hermite normal form with a unimodular transformation `U`, `U · A = [H; 0]`.
pub fn hermite_with_transform(a: &Mat<Int>) -> Hermite {
    hermite_impl(a, true)
}

fn hermite_impl(a: &Mat<Int>, want_u: bool) -> Hermite {
    let (m, n) = (a.rows, a.cols);
    let mut rows = a.to_rows();
    let mut u: Vec<Vec<Int>> = if want_u {
        Mat::identity(&Integers, m).to_rows()
    } else {
        Vec::new()
    };
    let mut pivots = Vec::new();
    let mut r = 0;
    for c in 0..n {
        if r == m {
            break;
        }
        // Collect the gcd of column c (rows r..) in row r, by unimodular row operations.
        for i in r + 1..m {
            if rows[i][c].is_zero() {
                continue;
            }
            if rows[r][c].is_zero() {
                rows.swap(r, i);
                if want_u {
                    u.swap(r, i);
                }
                continue;
            }
            let (x, y) = (rows[r][c].clone(), rows[i][c].clone());
            let (q, rem) = y.div_rem(&x);
            if rem.is_zero() {
                let (top, bottom) = rows.split_at_mut(i);
                sub_multiple(&mut bottom[0], &top[r], &q, c);
                if want_u {
                    let (top, bottom) = u.split_at_mut(i);
                    sub_multiple(&mut bottom[0], &top[r], &q, 0);
                }
                continue;
            }
            let (g, s, t) = xgcd(&x, &y);
            let (xg, yg) = (x.div_rem(&g).0, y.div_rem(&g).0);
            // The row operation [s t; −y/g x/g] has determinant 1.
            let combine = |v: &mut [Vec<Int>], from: usize| {
                let (top, bottom) = v.split_at_mut(i);
                let (vr, vi) = (&mut top[r], &mut bottom[0]);
                for j in from..vr.len() {
                    let (a, b) = (vr[j].clone(), vi[j].clone());
                    vr[j] = s.mul(&a).add(&t.mul(&b));
                    vi[j] = xg.mul(&b).sub(&yg.mul(&a));
                }
            };
            combine(&mut rows, c);
            if want_u {
                combine(&mut u, 0);
            }
        }
        if rows[r][c].is_zero() {
            continue;
        }
        if rows[r][c].is_negative() {
            for x in &mut rows[r][c..] {
                *x = x.neg();
            }
            if want_u {
                for x in &mut u[r] {
                    *x = x.neg();
                }
            }
        }
        let piv = rows[r][c].clone();
        for i in 0..r {
            let q = rows[i][c].div_mod_floor(&piv).0;
            if q.is_zero() {
                continue;
            }
            let (top, bottom) = rows.split_at_mut(r);
            sub_multiple(&mut top[i], &bottom[0], &q, c);
            if want_u {
                let (top, bottom) = u.split_at_mut(r);
                sub_multiple(&mut top[i], &bottom[0], &q, 0);
            }
        }
        pivots.push(c);
        r += 1;
    }
    rows.truncate(r);
    Hermite {
        h: Mat::from_rows(n, rows).expect("rows of equal length"),
        pivots,
        transform: want_u.then(|| Mat::from_rows(m, u).expect("rows of equal length")),
    }
}

/// The Smith invariant factors `d_1 | d_2 | ⋯ | d_r` of an integer matrix (all positive; `r` is
/// the rank). The matrix is equivalent under unimodular row and column operations to the
/// diagonal matrix with these entries.
pub fn smith_invariants(a: &Mat<Int>) -> Vec<Int> {
    // The Hermite form has the same row lattice, hence the same invariants, and fewer rows.
    let h = hermite(a).h;
    let cols = h.cols;
    let mut m = h.to_rows();
    let rows = m.len();
    let mut out = Vec::with_capacity(rows);
    for t in 0..rows.min(cols) {
        loop {
            // Pivot: an entry of least absolute value in the trailing block.
            let mut best: Option<(usize, usize, Int)> = None;
            for (i, row) in m.iter().enumerate().skip(t) {
                for (j, x) in row.iter().enumerate().skip(t) {
                    if x.is_zero() {
                        continue;
                    }
                    let ax = x.abs();
                    if best.as_ref().is_none_or(|(_, _, b)| ax < *b) {
                        best = Some((i, j, ax));
                    }
                }
            }
            let Some((pi, pj, _)) = best else {
                return out;
            };
            m.swap(t, pi);
            for row in &mut m {
                row.swap(t, pj);
            }
            let piv = m[t][t].clone();
            let mut clean = true;
            for i in t + 1..rows {
                if m[i][t].is_zero() {
                    continue;
                }
                let q = m[i][t].div_mod_floor(&piv).0;
                let (top, bottom) = m.split_at_mut(i);
                sub_multiple(&mut bottom[0], &top[t], &q, t);
                clean &= m[i][t].is_zero();
            }
            for j in t + 1..cols {
                if m[t][j].is_zero() {
                    continue;
                }
                let q = m[t][j].div_mod_floor(&piv).0;
                for row in m.iter_mut().skip(t) {
                    if !row[t].is_zero() {
                        row[j] = row[j].sub(&q.mul(&row[t]));
                    }
                }
                clean &= m[t][j].is_zero();
            }
            if !clean {
                continue;
            }
            // The pivot must divide the whole trailing block; if not, add an offending row to
            // row t and reduce again (the new remainder is smaller than the pivot).
            let bad = (t + 1..rows).find(|&i| {
                m[i][t + 1..]
                    .iter()
                    .any(|x| !x.div_mod_floor(&piv).1.is_zero())
            });
            if let Some(i) = bad {
                let (top, bottom) = m.split_at_mut(i);
                sub_multiple(&mut top[t], &bottom[0], &Int::from(-1), t);
                continue;
            }
            out.push(piv.abs());
            break;
        }
    }
    out
}

/// A finitely generated abelian group `ℤ^free_rank ⊕ ℤ/d_1 ⊕ ⋯ ⊕ ℤ/d_k` with `1 < d_1 | ⋯ | d_k`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AbelianGroup {
    pub free_rank: usize,
    /// The torsion invariant factors, each greater than 1, each dividing the next.
    pub torsion: Vec<Int>,
}

impl fmt::Display for AbelianGroup {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts = Vec::new();
        match self.free_rank {
            0 => {}
            1 => parts.push("ℤ".to_string()),
            k => parts.push(format!("ℤ^{k}")),
        }
        parts.extend(self.torsion.iter().map(|d| format!("ℤ/{d}")));
        if parts.is_empty() {
            write!(f, "0")
        } else {
            write!(f, "{}", parts.join(" ⊕ "))
        }
    }
}

/// The abelian group `ℤⁿ / (row span of A)` (`n` = number of columns): free rank `n − rank`,
/// torsion the Smith invariant factors greater than 1.
pub fn quotient_group(a: &Mat<Int>) -> AbelianGroup {
    let inv = smith_invariants(a);
    AbelianGroup {
        free_rank: a.cols - inv.len(),
        torsion: inv.into_iter().filter(|d| !d.is_one()).collect(),
    }
}

/// A ℤ-basis of the integer kernel `{x ∈ ℤⁿ : A x = 0}`, in Hermite normal form.
///
/// With `U · Aᵀ = [H; 0]` (`U` unimodular), the rows of `U` beyond the rank span exactly the
/// integer vectors annihilated by `A`.
pub fn kernel_z(a: &Mat<Int>) -> Vec<Vec<Int>> {
    let n = a.cols;
    let herm = hermite_with_transform(&a.transpose());
    let r = herm.pivots.len();
    let u = herm.transform.expect("requested");
    let basis: Vec<Vec<Int>> = (r..n).map(|i| u.row(i).to_vec()).collect();
    if basis.is_empty() {
        return basis;
    }
    hermite(&Mat::from_rows(n, basis).expect("rows of equal length"))
        .h
        .to_rows()
}

// ---------------------------------------------------------------------------------------------
// 𝔽_p: Montgomery arithmetic
// ---------------------------------------------------------------------------------------------

/// Montgomery arithmetic modulo an odd prime `p < 2⁶³`, with `R = 2⁶⁴`: `a` is stored as
/// `a·R mod p`, and a product needs one 128-bit multiplication and one reduction, with no
/// division.
#[derive(Clone, Copy, Debug)]
struct Mont {
    p: u64,
    /// `−p⁻¹ mod 2⁶⁴`.
    pinv: u64,
    /// `R² mod p`.
    r2: u64,
}

impl Mont {
    fn new(p: u64) -> Mont {
        debug_assert!(p % 2 == 1 && p < 1 << 63);
        // Newton iteration for p⁻¹ mod 2⁶⁴; p·p ≡ 1 mod 8 gives 3 correct bits to start.
        let mut inv = p;
        for _ in 0..6 {
            inv = inv.wrapping_mul(2u64.wrapping_sub(p.wrapping_mul(inv)));
        }
        let r = ((1u128 << 64) % p as u128) as u64;
        let r2 = ((r as u128 * r as u128) % p as u128) as u64;
        Mont {
            p,
            pinv: inv.wrapping_neg(),
            r2,
        }
    }

    /// `t · R⁻¹ mod p` for `t < p²`.
    #[inline]
    fn redc(&self, t: u128) -> u64 {
        let m = (t as u64).wrapping_mul(self.pinv);
        // t + m·p < 2¹²⁶ + 2¹²⁷ does not overflow and is divisible by 2⁶⁴; the quotient is < 2p.
        let u = ((t + m as u128 * self.p as u128) >> 64) as u64;
        // Branch-free `if u >= p { u - p } else { u }`: below p, u − p wraps past u.
        u.min(u.wrapping_sub(self.p))
    }

    #[inline]
    fn mul(&self, a: u64, b: u64) -> u64 {
        self.redc(a as u128 * b as u128)
    }

    #[inline]
    fn add(&self, a: u64, b: u64) -> u64 {
        let s = a + b;
        s.min(s.wrapping_sub(self.p))
    }

    fn enter(&self, a: u64) -> u64 {
        self.mul(a % self.p, self.r2)
    }

    fn leave(&self, a: u64) -> u64 {
        self.redc(a as u128)
    }

    fn inv(&self, a: u64) -> u64 {
        self.enter(inv_mod(self.leave(a), self.p))
    }

    /// Gauss–Jordan (`full`) or forward elimination of a matrix in Montgomery form, in place;
    /// returns the pivot columns. With `full`, the result is the reduced echelon form.
    fn eliminate(&self, d: &mut [u64], rows: usize, cols: usize, full: bool) -> Vec<usize> {
        let mut pivots = Vec::new();
        let mut r = 0;
        for c in 0..cols {
            if r == rows {
                break;
            }
            let Some(i) = (r..rows).find(|&i| d[i * cols + c] != 0) else {
                continue;
            };
            swap_rows(d, cols, i, r);
            let inv = self.inv(d[r * cols + c]);
            for x in &mut d[r * cols + c..(r + 1) * cols] {
                *x = self.mul(*x, inv);
            }
            let (before, rest) = d.split_at_mut(r * cols);
            let (prow, after) = rest.split_at_mut(cols);
            let prow = &prow[c..];
            let elim = |row: &mut [u64]| {
                let f = row[c];
                if f != 0 {
                    let nf = self.p - f;
                    for (x, &y) in row[c..].iter_mut().zip(prow) {
                        *x = self.add(*x, self.mul(nf, y));
                    }
                }
            };
            after.chunks_exact_mut(cols).for_each(elim);
            if full {
                before.chunks_exact_mut(cols).for_each(elim);
            }
            pivots.push(c);
            r += 1;
        }
        pivots
    }
}

/// `a⁻¹ mod p` for `a` prime to `p`.
fn inv_mod(a: u64, p: u64) -> u64 {
    let (mut r0, mut r1) = (p as i128, (a % p) as i128);
    let (mut t0, mut t1) = (0i128, 1i128);
    while r1 != 0 {
        let q = r0 / r1;
        (r0, r1) = (r1, r0 - q * r1);
        (t0, t1) = (t1, t0 - q * t1);
    }
    debug_assert_eq!(r0, 1, "not invertible");
    t0.rem_euclid(p as i128) as u64
}

#[inline]
fn mulmod(a: u64, b: u64, p: u64) -> u64 {
    ((a as u128 * b as u128) % p as u128) as u64
}

/// Elimination modulo a prime of residues; returns the (reduced, if `full`) matrix with
/// residues in `0..p` and the pivot columns.
fn fp_eliminate(
    p: u64,
    mut d: Vec<u64>,
    rows: usize,
    cols: usize,
    full: bool,
) -> (Vec<u64>, Vec<usize>) {
    if p.is_multiple_of(2) {
        let f = PrimeField::new(p).expect("p = 2 is prime");
        let e = rref_generic(
            &f,
            &Mat {
                rows,
                cols,
                data: d.into_iter().map(|x| x % p).collect(),
            },
        )
        .expect("ℤ/2 is a field");
        return (e.rref.data, e.pivots);
    }
    let mo = Mont::new(p);
    for x in &mut d {
        *x = mo.enter(*x);
    }
    let pivots = mo.eliminate(&mut d, rows, cols, full);
    for x in &mut d {
        *x = mo.leave(*x);
    }
    (d, pivots)
}

/// The reduced row echelon form over ℤ/p, on machine words (entries are reduced mod p first).
pub fn rref_mod_p(field: &PrimeField, a: &Mat<u64>) -> Echelon<u64> {
    let (d, pivots) = fp_eliminate(field.modulus(), a.data.clone(), a.rows, a.cols, true);
    Echelon {
        rref: Mat {
            rows: a.rows,
            cols: a.cols,
            data: d,
        },
        pivots,
        transform: None,
    }
}

/// The rank over ℤ/p, on machine words.
pub fn rank_mod_p(field: &PrimeField, a: &Mat<u64>) -> usize {
    fp_eliminate(field.modulus(), a.data.clone(), a.rows, a.cols, false)
        .1
        .len()
}

// ---------------------------------------------------------------------------------------------
// ℚ: multimodular elimination
// ---------------------------------------------------------------------------------------------

/// Primes below 2⁶², decreasing.
fn primes_62() -> impl Iterator<Item = u64> {
    let mut n = (1u64 << 62) + 1;
    std::iter::from_fn(move || {
        loop {
            n -= 2;
            if is_prime_u64(n) {
                return Some(n);
            }
        }
    })
}

/// Each row multiplied by the least common multiple of its denominators (this changes neither
/// the row space nor the kernel), as integers, row by row.
fn clear_denominators(a: &Mat<Rat>) -> Vec<Int> {
    let mut out = Vec::with_capacity(a.data.len());
    for i in 0..a.rows {
        let row = a.row(i);
        let l = row.iter().fold(Int::ONE, |l, x| {
            if x.denom().is_one() {
                l
            } else {
                lcm(&l, x.denom())
            }
        });
        out.extend(row.iter().map(|x| x.numer().mul(&l.div_rem(x.denom()).0)));
    }
    out
}

fn int_bits(x: &Int) -> u64 {
    match x {
        Int::Small(s) => 64 - s.unsigned_abs().leading_zeros() as u64,
        Int::Big(b) => b.bits(),
    }
}

/// How many primes to try before falling back to elimination over ℚ: enough for rational
/// reconstruction of quotients of minors under the Hadamard bound H (a modulus above 2H²),
/// plus a margin for unlucky primes and the consistency check.
fn prime_budget(ints: &[Int], m: usize, n: usize) -> usize {
    let half_log_n = 0.5 * (n.max(1) as f64).log2();
    let log2h: f64 = (0..m)
        .map(|i| {
            let bits = ints[i * n..(i + 1) * n]
                .iter()
                .map(int_bits)
                .max()
                .unwrap_or(0);
            if bits == 0 {
                0.0
            } else {
                bits as f64 + half_log_n
            }
        })
        .sum();
    ((2.0 * log2h + 2.0) / 61.0).ceil() as usize + 10
}

/// Orders pivot lists of reductions modulo different primes: `Less` means `a` is better
/// (higher rank, then lexicographically smaller). Modulo any prime the pivot list is at best the
/// one over ℚ.
fn cmp_pivots(a: &[usize], b: &[usize]) -> Ordering {
    b.len().cmp(&a.len()).then_with(|| a.cmp(b))
}

/// `n/d` in lowest terms with `|n|, d ≤ N` and `n ≡ d·u (mod M)`, if one exists; `u` in
/// `0..M`, `2N² < M` (so that it is unique).
fn rational_reconstruction(
    u: &BigInt,
    modulus: &BigInt,
    bound: &BigInt,
) -> Option<(BigInt, BigInt)> {
    let (mut r0, mut r1) = (modulus.clone(), u.clone());
    let (mut t0, mut t1) = (BigInt::zero(), BigInt::one());
    while &r1 > bound {
        let (q, r2) = r0.div_rem(&r1);
        let t2 = &t0 - &q * &t1;
        (r0, r1) = (r1, r2);
        (t0, t1) = (t1, t2);
    }
    if t1.is_zero() || &t1.abs() > bound {
        return None;
    }
    let (n, d) = if t1.is_negative() {
        (-r1, -t1)
    } else {
        (r1, t1)
    };
    n.gcd(&d).is_one().then_some((n, d))
}

/// Rational numbers `nums[e] / den` with a common denominator.
struct Candidate {
    den: BigInt,
    nums: Vec<BigInt>,
}

/// Rational numbers with the given residues modulo the primes (`residues[j][idx]` modulo
/// `primes[j]`), by Garner's mixed-radix Chinese remaindering and rational reconstruction, over
/// one common denominator; `None` if some value does not reconstruct within the bound
/// `N = ⌊√(M/2)⌋`.
///
/// A running common denominator `D` is kept: if `D·x` has a symmetric residue `y` with
/// `|y| ≤ N`, the value is `y/D` without a reconstruction (by uniqueness, since `D ≤ N`).
#[allow(clippy::needless_range_loop)]
fn reconstruct(primes: &[u64], residues: &[Vec<u64>]) -> Option<Candidate> {
    let k = primes.len();
    let count = residues[0].len();
    let monts: Vec<Mont> = primes.iter().map(|&p| Mont::new(p)).collect();
    let modulus = primes
        .iter()
        .fold(BigInt::one(), |acc, &p| acc * BigInt::from(p));
    let half: BigInt = &modulus >> 1usize;
    let bound = half.sqrt();
    // Garner constants p_i⁻¹ mod p_j in Montgomery form, so that a Montgomery product with a
    // residue in ordinary form gives an ordinary residue.
    let garner: Vec<Vec<u64>> = (0..k)
        .map(|j| {
            (0..j)
                .map(|i| monts[j].enter(inv_mod(primes[i], primes[j])))
                .collect()
        })
        .collect();
    let mut den = BigInt::one();
    // D mod p_j, in Montgomery form.
    let mut den_res: Vec<u64> = monts.iter().map(|mo| mo.enter(1)).collect();
    let mut digits = vec![0u64; k];
    // Numerators over the denominator current when they were found; `den_changes` records
    // (first index, denominator) at each change.
    let mut nums: Vec<BigInt> = Vec::with_capacity(count);
    let mut den_changes: Vec<(usize, BigInt)> = Vec::new();
    for idx in 0..count {
        for j in 0..k {
            let (pj, mo) = (primes[j], &monts[j]);
            let mut t = mo.mul(residues[j][idx], den_res[j]);
            for (i, &g) in garner[j].iter().enumerate() {
                // digits[i] < p_i < 2⁶² < 2 p_j
                let v = digits[i].min(digits[i].wrapping_sub(pj));
                t = mo.mul(if t >= v { t - v } else { t + pj - v }, g);
            }
            digits[j] = t;
        }
        if digits.iter().all(|&d| d == 0) {
            nums.push(BigInt::zero());
            continue;
        }
        let mut y = BigInt::from(digits[k - 1]);
        for j in (0..k - 1).rev() {
            y = y * primes[j] + digits[j];
        }
        if y > half {
            y -= &modulus;
        }
        if y.abs() <= bound {
            nums.push(y);
            continue;
        }
        if y.is_negative() {
            y += &modulus;
        }
        let (num, d2) = rational_reconstruction(&y, &modulus, &bound)?;
        let new_den = &den * &d2;
        if new_den > bound {
            return None;
        }
        den = new_den;
        den_res = monts
            .iter()
            .zip(primes)
            .map(|(mo, &p)| mo.enter((&den % p).to_u64().expect("residue fits")))
            .collect();
        den_changes.push((idx, den.clone()));
        nums.push(num);
    }
    // Bring every numerator over the final denominator.
    let mut prev_den = BigInt::one();
    let mut start = 0;
    for (at, d) in den_changes {
        let factor = &den / &prev_den;
        for x in &mut nums[start..at] {
            if !x.is_zero() {
                *x *= &factor;
            }
        }
        // The entry at `at` was found as num / (d2·previous D) = num / d.
        start = at;
        prev_den = d;
    }
    let factor = &den / &prev_den;
    if !factor.is_one() {
        for x in &mut nums[start..] {
            if !x.is_zero() {
                *x *= &factor;
            }
        }
    }
    Some(Candidate { den, nums })
}

/// True if the candidate values reduce to the residues modulo `p`.
fn agrees_mod(c: &Candidate, res: &[u64], p: u64) -> bool {
    let d = (&c.den % p).to_u64().expect("residue fits");
    d != 0
        && c.nums.iter().zip(res).all(|(x, &r)| {
            let xm = x
                .mod_floor(&BigInt::from(p))
                .to_u64()
                .expect("residue fits");
            xm == mulmod(r, d, p)
        })
}

fn mul_int(a: &BigInt, b: &Int) -> BigInt {
    match b {
        Int::Small(s) => a * *s,
        Int::Big(x) => a * x,
    }
}

/// Exact check over ℚ that every kernel vector given by a candidate reduced echelon form
/// (pivots; entries of the pivot rows in the free columns, row by row, over the common
/// denominator `den`) is annihilated by the integer matrix: for the free column `f` with
/// numerators `w_i`, the kernel vector is `(den·e_f − Σ_i w_i e_{p_i}) / den`.
fn verify_kernel(ints: &[Int], n: usize, pivots: &[usize], free: &[usize], c: &Candidate) -> bool {
    let nf = free.len();
    for (t, &f) in free.iter().enumerate() {
        for row in ints.chunks_exact(n) {
            let mut acc = -mul_int(&c.den, &row[f]);
            for (i, &p) in pivots.iter().enumerate() {
                let w = &c.nums[i * nf + t];
                if !w.is_zero() && !row[p].is_zero() {
                    acc += mul_int(w, &row[p]);
                }
            }
            if !acc.is_zero() {
                return false;
            }
        }
    }
    true
}

/// The reduced echelon form of an integer matrix over ℚ, multimodularly: pivots, and the
/// entries of the pivot rows in the free columns (row by row). `None` if no certified answer
/// was found within the prime budget.
fn multimodular_rref(ints: &[Int], m: usize, n: usize) -> Option<(Vec<usize>, Candidate)> {
    let budget = prime_budget(ints, m, n);
    let mut primes: Vec<u64> = Vec::new();
    let mut residues: Vec<Vec<u64>> = Vec::new();
    let mut best: Option<Vec<usize>> = None;
    let mut candidate: Option<Candidate> = None;
    for p in primes_62().take(budget) {
        let d: Vec<u64> = ints.iter().map(|x| x.rem_u64(p)).collect();
        let (d, piv) = fp_eliminate(p, d, m, n, true);
        match best.as_deref().map(|b| cmp_pivots(&piv, b)) {
            Some(Ordering::Greater) => continue,
            Some(Ordering::Equal) => {}
            _ => {
                best = Some(piv.clone());
                primes.clear();
                residues.clear();
                candidate = None;
            }
        }
        let free = free_columns(&piv, n);
        let vals: Vec<u64> = (0..piv.len())
            .flat_map(|i| free.iter().map(move |&f| i * n + f))
            .map(|ix| d[ix])
            .collect();
        if vals.is_empty() {
            // The echelon form is determined by the pivots alone.
            let c = Candidate {
                den: BigInt::one(),
                nums: Vec::new(),
            };
            if verify_kernel(ints, n, &piv, &free, &c) {
                return Some((piv, c));
            }
            continue;
        }
        if let Some(c) = candidate.take()
            && agrees_mod(&c, &vals, p)
            && verify_kernel(ints, n, &piv, &free, &c)
        {
            return Some((piv, c));
        }
        primes.push(p);
        residues.push(vals);
        candidate = reconstruct(&primes, &residues);
    }
    None
}

/// The reduced row echelon form over ℚ, computed modulo several 62-bit primes.
///
/// Rows are cleared of denominators; the reduced echelon form is computed modulo primes (a prime
/// whose pivots are worse than another's is discarded: its rank is too small or a pivot minor
/// vanishes there); the entries are lifted by Chinese remaindering and rational reconstruction
/// once they stabilize (agree modulo one further prime); and the result is checked exactly over
/// ℚ, which certifies it (see the module documentation). If no certified answer appears within a
/// Hadamard-bound budget of primes, the form is computed by elimination over ℚ.
pub fn rref_q(a: &Mat<Rat>) -> Echelon<Rat> {
    let (m, n) = (a.rows, a.cols);
    if m == 0 || n == 0 {
        return Echelon {
            rref: a.clone(),
            pivots: Vec::new(),
            transform: None,
        };
    }
    let ints = clear_denominators(a);
    let Some((pivots, cand)) = multimodular_rref(&ints, m, n) else {
        return rref_generic(&Rationals, a).expect("ℚ is a field");
    };
    let den = Int::from(cand.den);
    let free = free_columns(&pivots, n);
    let nf = free.len();
    let mut rref = Mat::zeros(&Rationals, m, n);
    for (i, &p) in pivots.iter().enumerate() {
        rref.set(i, p, Rat::ONE);
        for (t, &f) in free.iter().enumerate() {
            let x = &cand.nums[i * nf + t];
            if !x.is_zero() {
                rref.set(i, f, Rat::new(Int::from(x), den.clone()));
            }
        }
    }
    Echelon {
        rref,
        pivots,
        transform: None,
    }
}

/// The rank over ℚ. One prime gives a lower bound `r`; if `r` is the number of rows or columns
/// it is the rank. Otherwise the reduced echelon form of `A` or of `Aᵀ` (whichever has fewer
/// entries to reconstruct, `r·(cols − r)` against `r·(rows − r)`) is computed and certified
/// multimodularly, as in [`rref_q`].
pub fn rank_q(a: &Mat<Rat>) -> usize {
    let (m, n) = (a.rows, a.cols);
    if m == 0 || n == 0 {
        return 0;
    }
    let ints = clear_denominators(a);
    let p = primes_62().next().expect("infinitely many primes");
    let d = ints.iter().map(|x| x.rem_u64(p)).collect();
    let r = fp_eliminate(p, d, m, n, false).1.len();
    if r == m.min(n) {
        return r;
    }
    let found = if r * (m - r) < r * (n - r) {
        // Aᵀ with its columns scaled (by clearing A's rows) has the rank of A.
        let t = Mat {
            rows: m,
            cols: n,
            data: ints,
        }
        .transpose();
        multimodular_rref(&t.data, n, m)
    } else {
        multimodular_rref(&ints, m, n)
    };
    match found {
        Some((pivots, _)) => pivots.len(),
        None => rank_bareiss(&Rationals, a).expect("ℚ is a field"),
    }
}

/// A basis of the right kernel over ℚ, multimodularly, in the standard form of
/// [`Echelon::kernel`].
pub fn kernel_q(a: &Mat<Rat>) -> Vec<Vec<Rat>> {
    rref_q(a).kernel(&Rationals)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ratfun::RationalFunctions;
    use crate::ring::GaussianIntegers;

    /// A deterministic xorshift generator for test matrices.
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
        fn range(&mut self, lo: i64, hi: i64) -> i64 {
            lo + (self.next() % (hi - lo + 1) as u64) as i64
        }
    }

    fn rand_int_mat<R: Ring>(
        ring: &R,
        rng: &mut Rng,
        m: usize,
        n: usize,
        lo: i64,
        hi: i64,
    ) -> Mat<R::Elem> {
        Mat::from_fn(m, n, |_, _| ring.int(rng.range(lo, hi)))
    }

    /// A random `m × n` matrix of rank at most `k` (a product `m × k` by `k × n`).
    fn low_rank<R: Ring>(ring: &R, rng: &mut Rng, m: usize, n: usize, k: usize) -> Mat<R::Elem> {
        let b = rand_int_mat(ring, rng, m, k, -3, 3);
        let c = rand_int_mat(ring, rng, k, n, -3, 3);
        b.mul(ring, &c).unwrap()
    }

    fn assert_kernel<R: Ring>(ring: &R, a: &Mat<R::Elem>, ker: &[Vec<R::Elem>]) {
        for v in ker {
            assert!(a.mul_vec(ring, v).unwrap().iter().all(|x| ring.is_zero(x)));
        }
    }

    #[test]
    fn matrix_basics() {
        let z = Integers;
        let a = Mat::from_ints(&z, &[&[1, 2, 3], &[4, 5, 6]]).unwrap();
        let b = Mat::from_ints(&z, &[&[1, 0], &[0, 1], &[1, 1]]).unwrap();
        let ab = a.mul(&z, &b).unwrap();
        assert_eq!(ab, Mat::from_ints(&z, &[&[4, 5], &[10, 11]]).unwrap());
        assert_eq!(a.transpose().nrows(), 3);
        assert_eq!(*a.transpose().get(2, 1), Int::from(6));
        assert_eq!(
            a.mul_vec(&z, &[z.int(1), z.int(1), z.int(1)]).unwrap(),
            vec![z.int(6), z.int(15)]
        );
        assert!(a.mul(&z, &a).is_err());
        assert!(Mat::from_ints(&z, &[&[1, 2], &[3]]).is_err());
        assert_eq!(Mat::identity(&z, 2).mul(&z, &ab).unwrap(), ab);
        assert_eq!(ring_name::<PolyRing<Rationals>>(), "PolyRing<Rationals>");
    }

    #[test]
    fn rank_and_kernel_over_q() {
        let q = Rationals;
        let a = Mat::from_ints(&q, &[&[1, 2, 3], &[2, 4, 6], &[1, 0, 1]]).unwrap();
        assert_eq!(rank(&q, &a).unwrap(), 2);
        let e = rref(&q, &a).unwrap();
        assert_eq!(e.pivots, vec![0, 1]);
        let ker = kernel(&q, &a).unwrap();
        assert_eq!(ker, vec![vec![q.int(-1), q.int(-1), q.int(1)]]);
        assert_eq!(row_space(&q, &a).unwrap().len(), 2);
        // The multimodular path agrees, also on this small example.
        assert_eq!(rref_q(&a), e);
        assert_eq!(rank_q(&a), 2);
        let t = rref_with_transform(&q, &a).unwrap();
        assert_eq!(t.transform.unwrap().mul(&q, &a).unwrap(), t.rref);
        assert_eq!(t.rref, e.rref);
    }

    #[test]
    fn rank_over_prime_fields() {
        let f7 = PrimeField::new(7).unwrap();
        let q = Rationals;
        // det = 7
        let a: [&[i64]; 2] = [&[1, 2], &[3, 13]];
        assert_eq!(rank(&q, &Mat::from_ints(&q, &a).unwrap()).unwrap(), 2);
        assert_eq!(rank(&f7, &Mat::from_ints(&f7, &a).unwrap()).unwrap(), 1);
        // Large enough for the word-size path: diag(a, a, ..., a).
        let n = 20;
        let m7 = Mat::from_fn(n, n, |i, j| {
            if i / 2 == j / 2 {
                f7.int(a[i % 2][j % 2])
            } else {
                0
            }
        });
        assert_eq!(rank(&f7, &m7).unwrap(), 10);
        assert_eq!(rank_mod_p(&f7, &m7), 10);
        let e = rref(&f7, &m7).unwrap();
        assert_eq!(e, rref_generic(&f7, &m7).unwrap());
        assert_eq!(rref_mod_p(&f7, &m7), e);
        let ker = kernel(&f7, &m7).unwrap();
        assert_eq!(ker.len(), 10);
        assert_kernel(&f7, &m7, &ker);
        // ℤ/2 and a large prime.
        let f2 = PrimeField::new(2).unwrap();
        let m2 = Mat::from_ints(&f2, &[&[1, 1], &[1, 1]]).unwrap();
        assert_eq!(rank(&f2, &m2).unwrap(), 1);
        assert_eq!(rank_mod_p(&f2, &m2), 1);
        let fb = PrimeField::new(primes_62().next().unwrap()).unwrap();
        let mut rng = Rng(7);
        let mb = low_rank(&fb, &mut rng, 12, 15, 9);
        assert_eq!(
            rank(&fb, &mb).unwrap(),
            rref_generic(&fb, &mb).unwrap().rank()
        );
        assert_eq!(rref(&fb, &mb).unwrap(), rref_generic(&fb, &mb).unwrap());
    }

    #[test]
    fn rank_and_kernel_over_q_of_q() {
        let k = RationalFunctions::new(Rationals, "q");
        let q = k.named("q").unwrap();
        let qi = k.inv(&q).unwrap();
        let a =
            Mat::from_rows(2, vec![vec![q.clone(), k.one()], vec![k.one(), qi.clone()]]).unwrap();
        assert_eq!(rank(&k, &a).unwrap(), 1);
        assert_eq!(kernel(&k, &a).unwrap(), vec![vec![k.neg(&qi), k.one()]]);
        // Quantum integers [n] = q^{n-1} + q^{n-3} + ⋯ + q^{1-n}.
        let qint = |n: i64| {
            let mut s = k.zero();
            for e in 0..n {
                let t = k.mul(&k.pow(&q, e as u32), &k.pow(&qi, (n - 1 - e) as u32));
                s = k.add(&s, &t);
            }
            s
        };
        // The Hankel matrix ([i + j + 1]) has rank 2: [n+1] = (q + q⁻¹)[n] − [n−1].
        let b = Mat::from_fn(4, 4, |i, j| qint((i + j + 1) as i64));
        assert_eq!(rank(&k, &b).unwrap(), 2);
        assert_eq!(inverse(&k, &b).unwrap(), None);
        let ker = kernel(&k, &b).unwrap();
        assert_eq!(ker.len(), 2);
        assert_kernel(&k, &b, &ker);
        let c = Mat::from_fn(
            3,
            3,
            |i, j| if i == j { qint(i as i64 + 2) } else { k.one() },
        );
        let ci = inverse(&k, &c).unwrap().unwrap();
        assert_eq!(ci.mul(&k, &c).unwrap(), Mat::identity(&k, 3));
        // Constant entries over ℚ(q) take the multimodular path and map back.
        let mut rng = Rng(3);
        let c = low_rank(&k, &mut rng, 10, 10, 6);
        assert_eq!(rank(&k, &c).unwrap(), rref_generic(&k, &c).unwrap().rank());
        let ker = kernel(&k, &c).unwrap();
        assert_eq!(ker, rref_generic(&k, &c).unwrap().kernel(&k));
        assert_kernel(&k, &c, &ker);
    }

    #[test]
    fn rank_over_z_and_gaussian_integers() {
        let z = Integers;
        let a = Mat::from_ints(&z, &[&[2, 4], &[6, 8], &[1, 2]]).unwrap();
        assert_eq!(rank(&z, &a).unwrap(), 2);
        assert_eq!(rank_bareiss(&z, &a).unwrap(), 2);
        let mut rng = Rng(11);
        for (m, n, k) in [(9, 12, 5), (12, 9, 9), (10, 10, 10)] {
            let b = low_rank(&z, &mut rng, m, n, k);
            let r = rank(&z, &b).unwrap();
            assert_eq!(r, rank_bareiss(&z, &b).unwrap());
            assert_eq!(r, rank_q(&b.map(|x| Rat::from_int(x.clone()))));
        }
        let g = GaussianIntegers;
        let i = g.sqrt_minus_one().unwrap();
        // [[1, i], [i, -1]]: the second row is i times the first.
        let m = Mat::from_rows(
            2,
            vec![vec![g.one(), i.clone()], vec![i.clone(), g.int(-1)]],
        )
        .unwrap();
        assert_eq!(rank(&g, &m).unwrap(), 1);
        let m2 = Mat::from_rows(
            2,
            vec![vec![g.int(2), i.clone()], vec![i.clone(), g.int(3)]],
        )
        .unwrap();
        assert_eq!(rank(&g, &m2).unwrap(), 2);
    }

    #[test]
    fn bareiss_over_polynomials_agrees_with_rational_functions() {
        let zq = PolyRing::new(Integers, &["q"]);
        let k = RationalFunctions::new(Rationals, "q");
        let kq = k.named("q").unwrap();
        let to_k = |p: &Poly<Int>| {
            let mut acc = k.zero();
            for (mono, c) in &p.terms {
                let t = k.mul(
                    &k.pow(&kq, mono.0[0] as u32),
                    &k.rational(&c.to_bigint(), &BigInt::one()).unwrap(),
                );
                acc = k.add(&acc, &t);
            }
            acc
        };
        let mut rng = Rng(5);
        let q = zq.var(0);
        let mut rand_poly = || {
            let mut acc = zq.zero();
            for e in 0..3 {
                let t = zq.mul(&zq.pow(&q, e), &zq.int(rng.range(-2, 2)));
                acc = zq.add(&acc, &t);
            }
            acc
        };
        for (m, n, kk) in [(4, 5, 2), (5, 5, 5), (6, 4, 3), (5, 6, 4)] {
            let b = Mat::from_fn(m, kk, |_, _| rand_poly());
            let c = Mat::from_fn(kk, n, |_, _| rand_poly());
            let a = b.mul(&zq, &c).unwrap();
            let r = rank_bareiss(&zq, &a).unwrap();
            assert_eq!(r, rank(&zq, &a).unwrap());
            let ak = a.map(to_k);
            assert_eq!(r, rank(&k, &ak).unwrap());
            // The reduced echelon form over ℚ(q) has the same rank, and its kernel is one.
            let e = rref(&k, &ak).unwrap();
            assert_eq!(e.rank(), r);
            assert_kernel(&k, &ak, &e.kernel(&k));
            assert!(r <= kk);
        }
        // ℚ[q] goes through Bareiss too.
        let qq = PolyRing::new(Rationals, &["q"]);
        let x = qq.var(0);
        let x1 = qq.add(&x, &qq.one());
        let a = Mat::from_rows(
            2,
            vec![
                vec![x.clone(), x1.clone()],
                vec![qq.mul(&x, &x), qq.mul(&x, &x1)],
            ],
        )
        .unwrap();
        assert_eq!(rank(&qq, &a).unwrap(), 1);
    }

    #[test]
    fn laurent_rank() {
        let mut lq = PolyRing::new(Rationals, &["q"]);
        lq.laurent[0] = true;
        let q = lq.var(0);
        let qi = lq.inv(&q).unwrap();
        let one = lq.one();
        let q2 = lq.mul(&q, &q);
        // Rows (q⁻¹, 1, q) and (1, q, q²) are proportional; (1, q² + 1, q⁻²) is not.
        let a = Mat::from_rows(
            3,
            vec![
                vec![qi.clone(), one.clone(), q.clone()],
                vec![one.clone(), q.clone(), q2.clone()],
                vec![one.clone(), lq.add(&q2, &one), lq.mul(&qi, &qi)],
            ],
        )
        .unwrap();
        assert_eq!(rank_poly(&lq, &a).unwrap(), 2);
        // Plain Bareiss needs exact division by non-units, which the Laurent ring refuses.
        let b = Mat::from_rows(
            3,
            vec![
                vec![lq.add(&q, &one), one.clone(), q.clone()],
                vec![one.clone(), lq.add(&q2, &one), q.clone()],
                vec![q.clone(), one.clone(), lq.add(&q, &q2)],
            ],
        )
        .unwrap();
        let err = rank(&lq, &b).unwrap_err();
        assert!(
            err.message.contains("PolyRing<Rationals>"),
            "{}",
            err.message
        );
        assert_eq!(rank_poly(&lq, &b).unwrap(), 3);
        // Skew polynomial rings are not domains.
        let sk = PolyRing::skew(Rationals, &["x", "y"]);
        assert!(rank_poly(&sk, &Mat::identity(&sk, 2)).is_err());
    }

    #[test]
    fn hermite_and_smith() {
        let z = Integers;
        let a = Mat::from_ints(&z, &[&[2, 4], &[6, 8]]).unwrap();
        let h = hermite_with_transform(&a);
        assert_eq!(h.h, Mat::from_ints(&z, &[&[2, 0], &[0, 4]]).unwrap());
        assert_eq!(h.transform.unwrap().mul(&z, &a).unwrap(), h.h);
        assert_eq!(smith_invariants(&a), vec![Int::from(2), Int::from(4)]);
        let w = Mat::from_ints(&z, &[&[2, 4, 4], &[-6, 6, 12], &[10, -4, -16]]).unwrap();
        assert_eq!(
            smith_invariants(&w),
            vec![Int::from(2), Int::from(6), Int::from(12)]
        );
        // diag(2, 6) in ℤ³, disguised by unimodular row and column operations.
        let d = Mat::from_ints(&z, &[&[2, 0, 0], &[0, 6, 0]]).unwrap();
        let u = Mat::from_ints(&z, &[&[1, 1], &[2, 3]]).unwrap();
        let v = Mat::from_ints(&z, &[&[1, 2, 3], &[0, 1, 4], &[5, 11, 36]]).unwrap();
        let g = u.mul(&z, &d).unwrap().mul(&z, &v).unwrap();
        let grp = quotient_group(&g);
        assert_eq!(grp.free_rank, 1);
        assert_eq!(grp.torsion, vec![Int::from(2), Int::from(6)]);
        assert_eq!(grp.to_string(), "ℤ ⊕ ℤ/2 ⊕ ℤ/6");
        assert_eq!(quotient_group(&Mat::identity(&z, 2)).to_string(), "0");
        // The kernel lattice of (1 2 3) has rank 2 and is saturated.
        let k = kernel_z(&Mat::from_ints(&z, &[&[1, 2, 3]]).unwrap());
        assert_eq!(k.len(), 2);
        let km = Mat::from_rows(3, k).unwrap();
        let col = Mat::from_ints(&z, &[&[1], &[2], &[3]]).unwrap();
        assert!(km.mul(&z, &col).unwrap().data().iter().all(Int::is_zero));
        assert!(quotient_group(&km).torsion.is_empty());
        // 2x + 4y = 0: the kernel lattice is spanned by (2, -1).
        let k2 = kernel_z(&Mat::from_ints(&z, &[&[2, 4]]).unwrap());
        assert_eq!(k2, vec![vec![Int::from(2), Int::from(-1)]]);
        // Random cross-checks: the transform, the rank, and the divisibility chain.
        let mut rng = Rng(9);
        for _ in 0..5 {
            let a = rand_int_mat(&z, &mut rng, 4, 5, -6, 6);
            let h = hermite_with_transform(&a);
            let ua = h.transform.clone().unwrap().mul(&z, &a).unwrap();
            let len = h.h.data().len();
            assert_eq!(&ua.data()[..len], h.h.data());
            assert!(ua.data()[len..].iter().all(Int::is_zero));
            let inv = smith_invariants(&a);
            assert_eq!(inv.len(), rank(&z, &a).unwrap());
            assert!(inv.windows(2).all(|w| w[1].div_rem(&w[0]).1.is_zero()));
        }
    }

    #[test]
    fn solve_and_inverse() {
        let q = Rationals;
        let a = Mat::from_ints(&q, &[&[1, 1, 0], &[0, 1, 1]]).unwrap();
        let (x, ker) = solve(&q, &a, &[q.int(2), q.int(3)]).unwrap().unwrap();
        assert_eq!(a.mul_vec(&q, &x).unwrap(), vec![q.int(2), q.int(3)]);
        assert_eq!(ker.len(), 1);
        assert_kernel(&q, &a, &ker);
        let s = Mat::from_ints(&q, &[&[1, 2], &[2, 4]]).unwrap();
        assert_eq!(solve(&q, &s, &[q.int(1), q.int(3)]).unwrap(), None);
        assert!(solve(&q, &s, &[q.int(1), q.int(2)]).unwrap().is_some());
        assert_eq!(inverse(&q, &s).unwrap(), None);
        let b = Mat::from_ints(&q, &[&[2, 1], &[7, 4]]).unwrap();
        let bi = inverse(&q, &b).unwrap().unwrap();
        assert_eq!(bi, Mat::from_ints(&q, &[&[4, -1], &[-7, 2]]).unwrap());
        assert!(inverse(&q, &a).is_err());
        assert!(kernel(&Integers, &Mat::identity(&Integers, 2)).is_err());
        // Larger systems take the multimodular path.
        let mut rng = Rng(21);
        let big = rand_int_mat(&q, &mut rng, 12, 12, -9, 9);
        let bi = inverse(&q, &big).unwrap().unwrap();
        assert_eq!(bi.mul(&q, &big).unwrap(), Mat::identity(&q, 12));
        let lr = low_rank(&q, &mut rng, 10, 12, 7);
        let x0: Vec<Rat> = (0..12).map(|i| q.int(i - 5)).collect();
        let b = lr.mul_vec(&q, &x0).unwrap();
        let (x, ker) = solve(&q, &lr, &b).unwrap().unwrap();
        assert_eq!(lr.mul_vec(&q, &x).unwrap(), b);
        assert_eq!(ker.len(), 12 - rank(&q, &lr).unwrap());
        let mut b2 = b.clone();
        b2[0] = q.add(&b2[0], &q.one());
        // Not in the column space (rank 7 < 10 rows), for this seed.
        if rank(
            &q,
            &lr.hstack(&Mat::new(10, 1, b2.clone()).unwrap()).unwrap(),
        )
        .unwrap()
            > 7
        {
            assert_eq!(solve(&q, &lr, &b2).unwrap(), None);
        }
    }

    #[test]
    fn reducer() {
        let q = Rationals;
        let mut red = Reducer::new(q.clone(), 4);
        let v = |xs: &[i64]| xs.iter().map(|&x| q.int(x)).collect::<Vec<_>>();
        assert!(red.insert(v(&[0, 2, 4, 2])));
        assert!(red.insert(v(&[1, 1, 0, 0])));
        assert!(!red.insert(v(&[2, 4, 4, 2])));
        assert!(!red.insert(v(&[0, 0, 0, 0])));
        assert_eq!(red.rank(), 2);
        assert_eq!(red.pivots(), vec![0, 1]);
        assert_eq!(red.free_columns(), vec![2, 3]);
        // Fully reduced: pivot columns zero in the other rows, pivot entries 1.
        assert_eq!(red.basis()[0], v(&[1, 0, -2, -1]));
        assert_eq!(red.basis()[1], v(&[0, 1, 2, 1]));
        let w = red.reduce(&v(&[3, 5, 1, 1]));
        assert!(q.is_zero(&w[0]) && q.is_zero(&w[1]));
        assert_eq!(
            red.quotient_coords(&v(&[3, 5, 1, 1])),
            vec![q.int(-3), q.int(-1)]
        );
        assert!(red.contains(&v(&[1, 3, 4, 2])));
        assert!(red.insert(v(&[0, 0, 1, 0])));
        assert_eq!(red.pivots(), vec![0, 1, 2]);
        assert_eq!(red.basis()[0], v(&[1, 0, 0, -1]));
        let basis = Mat::from_rows(4, red.basis().to_vec()).unwrap();
        let from = Reducer::from_matrix(q.clone(), &basis).unwrap();
        assert_eq!(from.basis(), red.basis());
        // Over 𝔽_5.
        let f = PrimeField::new(5).unwrap();
        let mut r5 = Reducer::new(f.clone(), 2);
        assert!(r5.insert(vec![2, 1]));
        assert!(!r5.insert(vec![4, 2]));
        assert_eq!(r5.basis()[0], vec![1, 3]);
        assert_eq!(r5.ring(), &f);
    }

    /// Random rational matrices: multimodular against direct elimination over ℚ.
    #[test]
    fn multimodular_agrees_with_direct_elimination() {
        let q = Rationals;
        let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
        let mut cases: Vec<Mat<Rat>> = Vec::new();
        for &(m, n, k) in &[
            (8, 8, 8),
            (10, 12, 6),
            (12, 10, 4),
            (15, 15, 11),
            (6, 20, 6),
            (20, 6, 3),
            (9, 9, 0),
        ] {
            cases.push(low_rank(&q, &mut rng, m, n, k));
        }
        // Rational entries with large numerators and denominators (BigInt arithmetic, many
        // primes in the reconstruction).
        let big = |rng: &mut Rng| {
            let n = Int::from(rng.range(-1_000_000_000_000_000, 1_000_000_000_000_000));
            let d = Int::from(rng.range(1, 1_000_000_007));
            Rat::new(n, d)
        };
        let a = Mat::from_fn(7, 9, |_, _| big(&mut rng));
        // Rank deficient: append combinations of its rows.
        let mut rows = a.to_rows();
        for t in 0..3 {
            let c = big(&mut rng);
            let comb: Vec<Rat> = (0..9)
                .map(|j| q.add(&q.mul(&rows[t][j], &c), &rows[t + 1][j]))
                .collect();
            rows.push(comb);
        }
        cases.push(Mat::from_rows(9, rows).unwrap());
        // The Hilbert matrix.
        cases.push(Mat::from_fn(8, 8, |i, j| {
            Rat::new(Int::ONE, Int::from((i + j + 1) as i64))
        }));
        for a in &cases {
            let direct = rref_generic(&q, a).unwrap();
            assert_eq!(rref_q(a), direct);
            assert_eq!(rank_q(a), direct.rank());
            assert_eq!(rank(&q, a).unwrap(), direct.rank());
            let ker = kernel_q(a);
            assert_eq!(ker.len(), a.ncols() - direct.rank());
            assert_kernel(&q, a, &ker);
        }
    }

    /// Timing for the performance target (run with `--release -- --ignored --nocapture`).
    #[test]
    #[ignore]
    fn timing_rank_200() {
        use std::time::Instant;
        let q = Rationals;
        let mut rng = Rng(42);
        let n = 200;
        let a = rand_int_mat(&q, &mut rng, n, n, -5, 5);
        let t = Instant::now();
        let r = rank(&q, &a).unwrap();
        println!(
            "rank over QQ, random {n}x{n}, entries in -5..5: {r} in {:?}",
            t.elapsed()
        );
        let t = Instant::now();
        let r = rank_q(&a);
        println!("  rank_q directly: {r} in {:?}", t.elapsed());

        let f = PrimeField::new(primes_62().next().unwrap()).unwrap();
        let af = rand_int_mat(&f, &mut rng, n, n, -5, 5);
        let t = Instant::now();
        let r = rank(&f, &af).unwrap();
        println!("rank over GF({}): {r} in {:?}", f.modulus(), t.elapsed());
        let f3 = PrimeField::new(1_000_003).unwrap();
        let a3 = rand_int_mat(&f3, &mut rng, n, n, -5, 5);
        let t = Instant::now();
        let r = rank(&f3, &a3).unwrap();
        println!("rank over GF(1000003): {r} in {:?}", t.elapsed());

        let b = rand_int_mat(&q, &mut rng, n, 150, -5, 5);
        let c = rand_int_mat(&q, &mut rng, 150, n, -5, 5);
        let lr = b.mul(&q, &c).unwrap();
        let t = Instant::now();
        let r = rank(&q, &lr).unwrap();
        println!(
            "rank over QQ, {n}x{n} of rank 150 (product of random -5..5 factors): {r} in {:?}",
            t.elapsed()
        );
        let t = Instant::now();
        let k = kernel(&q, &lr).unwrap();
        println!(
            "  kernel over QQ (dimension {}) in {:?}",
            k.len(),
            t.elapsed()
        );
        let lrf = lr.map(|x| f.int(0) + x.numer().rem_u64(f.modulus()));
        let t = Instant::now();
        let r = rank(&f, &lrf).unwrap();
        println!("rank over GF(p) of the same: {r} in {:?}", t.elapsed());
    }
}
