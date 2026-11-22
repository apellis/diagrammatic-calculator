//! Randomized differential tests of `complex.deform` and `module.head` against brute-force
//! oracles (`docs/complexes.md`, "Parameters and deformations" and "Heads").
//!
//! The oracles share none of the code under test beyond `simplify`, which straightens one term:
//! every composite is written out as text, straightened, and read back as coordinates on the
//! monomials of the normal form, with `h` treated as an ordinary scalar and the truncation applied
//! here. The linear algebra (ranks, kernels, consistency) is done in this file over `ℚ`.
//!
//! - `complex.deform`: at each order `m` reported, the system
//!   `[d_0 X_m + X_m d_0]_0 = −[d²]_m` (unknown entries only) is rebuilt from the complex with the
//!   reported lower-order corrections, and its solvability, kernel dimension, first inconsistent
//!   square and printed obstruction are compared with the output. Above order 1 the system
//!   depends on the corrections chosen below, so the oracle checks the solver's own path, not
//!   every possible choice. The returned complex is checked to square to zero modulo `h^{N+1}`
//!   and to differ from the input only at unknown entries, by multiples of `h`.
//! - `module.head`: `End⁰(X)` of each summand object is built from its Hom basis and composites;
//!   `J` is the radical of the trace form, `dim End⁰/J` its rank. The head's graded dimensions
//!   are the ranks of `f ↦ (f ∘ g mod J)_g`, and the simplicity note must appear exactly when
//!   `dim End⁰/J > 1`.
//! - complexes (`complex_operations_agree_with_brute_force`): random one-dimensional complexes in
//!   A1, A2 (with `h` in the relations), B2, and A3 (two red strands), and A2 with weights theta,
//!   over one and two black strands, with `h` of degree 2, 0, and −2 in the maps. Third terms are
//!   random cycles, and contractible pairs are added. The oracle checks `complex.check`,
//!   `complex.homology` (dimensions and representatives), `complex.euler`, `complex.minimal`
//!   (each entry against the trace form of `End⁰`), `complex.gauss` (homology kept; minimal when
//!   every `End⁰` is local), `complex.kernel` (in the kernel, generating it, none redundant),
//!   `chainmap.check` and `homotopy.check` (on `d h + h d`, plus 1 or not, and a perturbation),
//!   `complex.cone` (entries `[[−d, 0], [f, d]]`; acyclic for a map homotopic to 1),
//!   `complex.totalize` (the square `C --f--> C`, block by block, acyclic for `f` homotopic to
//!   1), `complex.shift`, `homotopy.find` and `chainmap.solve` (existence and freedom against the
//!   linear system in single-entry maps), `complex.hom` (cohomology of `Hom(C, C)` degree by
//!   degree), `module.check` (graded dimensions of the cokernel), and `resolution.start`, `.extend`, and
//!   `.check` (each new term covering the kernel at the far end, none redundant).

#![allow(clippy::needless_range_loop)] // the exact linear algebra below indexes by row and column

use diagcalc_core::ops;
use proptest::prelude::*;
use proptest::test_runner::RngSeed;

/// The random tests draw from a fixed seed, so every run checks the same cases and a failure in
/// CI can be reproduced exactly. Change the seed deliberately to explore other cases.
const FIXED_SEED: RngSeed = RngSeed::Fixed(0x6469_6167_6361_6c63);
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

// ---------------------------------------------------------------------------------------------
// exact rationals

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Q(i128, i128);

fn gcd(a: i128, b: i128) -> i128 {
    if b == 0 { a.abs() } else { gcd(b, a % b) }
}

impl Q {
    fn new(n: i128, d: i128) -> Q {
        assert!(d != 0);
        let g = gcd(n, d).max(1);
        let s = if d < 0 { -1 } else { 1 };
        Q(s * n / g, s * d / g)
    }
    fn int(n: i128) -> Q {
        Q(n, 1)
    }
    fn zero(self) -> bool {
        self.0 == 0
    }
    fn add(self, o: Q) -> Q {
        Q::new(self.0 * o.1 + o.0 * self.1, self.1 * o.1)
    }
    fn neg(self) -> Q {
        Q(-self.0, self.1)
    }
    fn sub(self, o: Q) -> Q {
        self.add(o.neg())
    }
    fn mul(self, o: Q) -> Q {
        Q::new(self.0 * o.0, self.1 * o.1)
    }
    fn div(self, o: Q) -> Q {
        Q::new(self.0 * o.1, self.1 * o.0)
    }
    fn parse(s: &str) -> Option<Q> {
        match s.split_once('/') {
            Some((n, d)) => Some(Q::new(n.parse().ok()?, d.parse().ok()?)),
            None => s.parse().ok().map(Q::int),
        }
    }
}

/// The rank of a matrix over `ℚ` (rows of equal length).
fn rank(rows: &[Vec<Q>]) -> usize {
    let mut m: Vec<Vec<Q>> = rows.to_vec();
    let cols = m.first().map_or(0, Vec::len);
    let mut r = 0;
    for c in 0..cols {
        let Some(p) = (r..m.len()).find(|&i| !m[i][c].zero()) else {
            continue;
        };
        m.swap(r, p);
        for i in 0..m.len() {
            if i != r && !m[i][c].zero() {
                let f = m[i][c].div(m[r][c]);
                for k in c..cols {
                    let v = m[r][k].mul(f);
                    m[i][k] = m[i][k].sub(v);
                }
            }
        }
        r += 1;
    }
    r
}

// ---------------------------------------------------------------------------------------------
// terms as coordinates

/// A morphism as coordinates on (power of `h`, monomial of the normal form without `h`).
type Lin = BTreeMap<(u32, String), Q>;

fn call(op: &str, req: Value) -> Value {
    let v = ops::call(op, &req);
    assert_eq!(v["status"], "ok", "{op}: {req:#} gives {v:#}");
    v
}

/// Splits at top-level occurrences of `sep` (outside parentheses).
fn split_top(s: &str, sep: char) -> Vec<String> {
    let mut out = vec![String::new()];
    let mut depth = 0i32;
    for ch in s.chars() {
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            _ => {}
        }
        if ch == sep && depth == 0 {
            out.push(String::new());
        } else {
            out.last_mut().unwrap().push(ch);
        }
    }
    out.into_iter().filter(|t| !t.is_empty()).collect()
}

/// Reads a normal form `c₁ m₁ + c₂ m₂ - …` printed by `simplify`. A coefficient may be a
/// polynomial in `h`, printed in parentheses: `(2 h - 1) x_1 e(i@1)`.
fn parse_normal_form(nf: &str) -> Lin {
    parse_sum(nf, false)
}

/// Reads a signed sum; with `scalar`, the monomials may be empty (a polynomial in `h`).
fn parse_sum(nf: &str, scalar: bool) -> Lin {
    let mut out = Lin::new();
    if nf.trim() == "0" {
        return out;
    }
    // signed pieces: a piece starts at the beginning or after " + " / " - " at depth 0
    let mut pieces: Vec<(bool, String)> = Vec::new();
    let mut depth = 0i32;
    let cs: Vec<char> = nf.trim().chars().collect();
    let mut cur = String::new();
    let mut neg = false;
    let mut i = 0;
    while i < cs.len() {
        let ch = cs[i];
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            _ => {}
        }
        if depth == 0
            && (ch == '+' || ch == '-')
            && i > 0
            && cs[i - 1] == ' '
            && cs.get(i + 1) == Some(&' ')
        {
            pieces.push((neg, cur.trim().to_string()));
            cur.clear();
            neg = ch == '-';
            i += 2;
            continue;
        }
        if i == 0 && ch == '-' {
            neg = true;
        } else {
            cur.push(ch);
        }
        i += 1;
    }
    pieces.push((neg, cur.trim().to_string()));
    for (neg, p) in pieces {
        let toks = split_top(&p, ' ');
        // the coefficient, as a polynomial in h
        let mut coeff: BTreeMap<u32, Q> = BTreeMap::from([(0, Q::int(1))]);
        let mut hpow = 0u32;
        let mut key = Vec::new();
        for (k, t) in toks.iter().enumerate() {
            if k == 0 {
                if let Some(c) = Q::parse(t) {
                    coeff = BTreeMap::from([(0, c)]);
                    continue;
                }
                if let Some(inner) = t.strip_prefix('(').and_then(|r| r.strip_suffix(')')) {
                    coeff = parse_sum(inner, true)
                        .into_iter()
                        .map(|((h, k), c)| {
                            assert!(k.is_empty(), "a coefficient with a monomial: {t:?}");
                            (h, c)
                        })
                        .collect();
                    continue;
                }
            }
            if t == "h" {
                hpow += 1;
            } else if let Some(e) = t.strip_prefix("h^") {
                hpow += e.parse::<u32>().expect("power of h");
            } else {
                key.push(t.clone());
            }
        }
        assert!(scalar || !key.is_empty(), "no monomial in {p:?} of {nf:?}");
        for (h, c) in coeff {
            let c = if neg { c.neg() } else { c };
            let e = out.entry((h + hpow, key.join(" "))).or_insert(Q::int(0));
            *e = e.add(c);
        }
    }
    out.retain(|_, c| !c.zero());
    out
}

/// Straightens `term` in `ctx` with `h` an ordinary scalar, then drops powers of `h` above `n`.
struct Oracle {
    ctx: String,
    n: u32,
    cache: BTreeMap<String, Lin>,
    hom: BTreeMap<(String, String, i64), Vec<String>>,
}

impl Oracle {
    fn new(ctx: &str, n: u32) -> Oracle {
        Oracle {
            ctx: ctx.to_string(),
            n,
            cache: BTreeMap::new(),
            hom: BTreeMap::new(),
        }
    }

    fn nf(&mut self, term: &str) -> Lin {
        if term.trim().is_empty() {
            return Lin::new();
        }
        if let Some(l) = self.cache.get(term) {
            return l.clone();
        }
        let v = call("simplify", json!({ "context": self.ctx, "term": term }));
        let mut l = parse_normal_form(v["normal_form"].as_str().unwrap());
        l.retain(|(h, _), _| *h <= self.n);
        self.cache.insert(term.to_string(), l.clone());
        l
    }

    /// `Σ_k (g_k)(f_k)`, straightened.
    fn sum_of_products(&mut self, pairs: &[(&str, &str)]) -> Lin {
        let t: Vec<String> = pairs
            .iter()
            .filter(|(g, f)| !g.is_empty() && !f.is_empty())
            .map(|(g, f)| format!("({g})({f})"))
            .collect();
        self.nf(&t.join(" + "))
    }

    /// The basis of `Hom` from `src` to `tgt` in `degree` printed by `hom.basis` (with its
    /// multiples of `h` when the context is truncated).
    fn hom(&mut self, src: &str, tgt: &str, degree: i64) -> Vec<String> {
        let k = (src.to_string(), tgt.to_string(), degree);
        if let Some(b) = self.hom.get(&k) {
            return b.clone();
        }
        let v = call(
            "hom.basis",
            json!({ "context": self.ctx, "source": src, "target": tgt, "degree": degree }),
        );
        let b: Vec<String> = v["degrees"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|d| d["degree"] == degree)
            .flat_map(|d| d["basis"].as_array().unwrap().clone())
            .map(|s| s.as_str().unwrap().to_string())
            .collect();
        self.hom.insert(k, b.clone());
        b
    }

    /// The part of that basis free of `h`.
    fn hom_free(&mut self, src: &str, tgt: &str, degree: i64) -> Vec<String> {
        let b = self.hom(src, tgt, degree);
        b.into_iter()
            .filter(|f| {
                let l = self.nf(f);
                !l.is_empty() && l.keys().all(|(h, _)| *h == 0)
            })
            .collect()
    }
}

fn lin_sub(a: &Lin, b: &Lin) -> Lin {
    let mut out = a.clone();
    for (k, c) in b {
        let e = out.entry(k.clone()).or_insert(Q::int(0));
        *e = e.sub(*c);
    }
    out.retain(|_, c| !c.zero());
    out
}

fn part(l: &Lin, m: u32) -> BTreeMap<String, Q> {
    l.iter()
        .filter(|((h, _), _)| *h == m)
        .map(|((_, k), c)| (k.clone(), *c))
        .collect()
}

// ---------------------------------------------------------------------------------------------
// a small deterministic generator driven by a proptest seed

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn pick<'a, T>(&mut self, xs: &'a [T]) -> &'a T {
        &xs[self.below(xs.len())]
    }
    fn chance(&mut self, num: usize, den: usize) -> bool {
        self.below(den) < num
    }
}

/// A random combination of `basis` with small integer coefficients, as text ("" for zero).
fn combination(r: &mut Rng, basis: &[String], density: usize) -> String {
    let mut s = String::new();
    for b in basis {
        if !r.chance(density, 4) {
            continue;
        }
        let c = *r.pick(&[1i64, -1, 2, -3]);
        let sign = if c < 0 { "-" } else { "+" };
        if s.is_empty() {
            s = format!("{}{} {b}", if c < 0 { "-" } else { "" }, c.abs());
        } else {
            s = format!("{s} {sign} {} {b}", c.abs());
        }
    }
    s
}

fn add_terms(a: &str, b: &str) -> String {
    match (a.is_empty(), b.is_empty()) {
        (true, _) => b.to_string(),
        (_, true) => a.to_string(),
        _ => match b.strip_prefix('-') {
            Some(rest) => format!("{a} - {rest}"),
            None => format!("{a} + {b}"),
        },
    }
}

// ---------------------------------------------------------------------------------------------
// complex.deform

/// A complex with terms at positions `lo, lo + 1, …, 0`; `diffs[t]` goes from `lo + t`.
#[derive(Clone, Debug)]
struct Cx {
    terms: Vec<Vec<(String, i64)>>,
    diffs: Vec<Vec<Vec<String>>>,
}

impl Cx {
    fn lo(&self) -> i64 {
        -(self.terms.len() as i64 - 1)
    }
    fn json(&self) -> Value {
        let lo = self.lo();
        json!({
            "terms": self.terms.iter().enumerate().map(|(t, ss)| json!({
                "at": lo + t as i64,
                "summands": ss.iter().map(|(o, s)| json!({ "object": o, "shift": s })).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
            "differentials": self.diffs.iter().enumerate().map(|(t, m)| json!({
                "from": lo + t as i64,
                "matrix": m.iter().map(|row| row.iter().map(|f| {
                    if f.is_empty() { Value::Null } else { Value::String(f.clone()) }
                }).collect::<Vec<_>>()).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
        })
    }
    /// Reads the differentials of a complex returned by an operation.
    fn with_diffs_from(&self, v: &Value) -> Cx {
        let mut out = self.clone();
        let lo = self.lo();
        for d in v["differentials"].as_array().unwrap() {
            let t = (d["from"].as_i64().unwrap() - lo) as usize;
            for (i, row) in d["matrix"].as_array().unwrap().iter().enumerate() {
                for (j, f) in row.as_array().unwrap().iter().enumerate() {
                    out.diffs[t][i][j] = f.as_str().unwrap_or("").to_string();
                }
            }
        }
        out
    }
    /// The degree of the entry `(i, j)` of the differential from `lo + t`.
    fn degree(&self, t: usize, i: usize, j: usize) -> i64 {
        self.terms[t][j].1 - self.terms[t + 1][i].1
    }
}

/// The system at order `m` (see the module documentation), rebuilt for the complex `c`.
struct OrderSystem {
    /// For each square (indexed by the source position `lo + t`): `[d²]_m` entrywise.
    dd: Vec<Vec<Vec<BTreeMap<String, Q>>>>,
    /// `[d²]_k` for `k < m`, nonzero entries only (should be none).
    lower: Vec<(usize, u32)>,
    /// The columns: the effect of each unknown basis vector `h^m Y` on the `h^m` coefficients.
    cols: Vec<BTreeMap<(usize, usize, usize, String), Q>>,
    /// `(t, i, j, Y)` for each column.
    vars: Vec<(usize, usize, usize, String)>,
}

fn order_system(
    o: &mut Oracle,
    c: &Cx,
    unknowns: &BTreeSet<(usize, usize, usize)>,
    m: u32,
    hdeg: i64,
) -> OrderSystem {
    let nt = c.terms.len();
    let mut dd = Vec::new();
    let mut lower = Vec::new();
    for t in 0..nt.saturating_sub(2) {
        let (np, nq, nr) = (c.terms[t].len(), c.terms[t + 1].len(), c.terms[t + 2].len());
        let mut sq = vec![vec![BTreeMap::new(); np]; nr];
        for (i, row) in sq.iter_mut().enumerate() {
            for (j, e) in row.iter_mut().enumerate() {
                let pairs: Vec<(&str, &str)> = (0..nq)
                    .map(|l| (c.diffs[t + 1][i][l].as_str(), c.diffs[t][l][j].as_str()))
                    .collect();
                let l = o.sum_of_products(&pairs);
                for k in 0..m {
                    if !part(&l, k).is_empty() {
                        lower.push((t, k));
                    }
                }
                *e = part(&l, m);
            }
        }
        dd.push(sq);
    }
    let mut cols = Vec::new();
    let mut vars = Vec::new();
    if m > 0 {
        for &(t, i, j) in unknowns {
            let deg = c.degree(t, i, j) - m as i64 * hdeg;
            let (src, tgt) = (c.terms[t][j].0.clone(), c.terms[t + 1][i].0.clone());
            for y in o.hom_free(&src, &tgt, deg) {
                let hy = format!("h^{m} {y}");
                let mut col = BTreeMap::new();
                // as the first map of the square from lo + t
                if t + 2 < nt {
                    for i2 in 0..c.terms[t + 2].len() {
                        let l = o.sum_of_products(&[(c.diffs[t + 1][i2][i].as_str(), hy.as_str())]);
                        for (k, v) in part(&l, m) {
                            col.insert((t, i2, j, k), v);
                        }
                    }
                }
                // as the second map of the square from lo + t - 1
                if t >= 1 {
                    for j2 in 0..c.terms[t - 1].len() {
                        let l = o.sum_of_products(&[(hy.as_str(), c.diffs[t - 1][j][j2].as_str())]);
                        for (k, v) in part(&l, m) {
                            let e = col.entry((t - 1, i, j2, k)).or_insert(Q::int(0));
                            *e = e.add(v);
                        }
                    }
                }
                col.retain(|_, v: &mut Q| !v.zero());
                cols.push(col);
                vars.push((t, i, j, y));
            }
        }
    }
    OrderSystem {
        dd,
        lower,
        cols,
        vars,
    }
}

impl OrderSystem {
    /// Rows: every coordinate that occurs, in squares `< upto`. Returns (rank of the coefficient
    /// matrix, rank with the right side `−[d²]_m` appended).
    fn ranks(&self, upto: usize) -> (usize, usize) {
        self.ranks_in(0, upto)
    }
    /// As `ranks`, for the squares `from ≤ t < upto` only.
    fn ranks_in(&self, from: usize, upto: usize) -> (usize, usize) {
        let mut keys: BTreeSet<(usize, usize, usize, String)> = BTreeSet::new();
        for col in &self.cols {
            keys.extend(col.keys().filter(|k| from <= k.0 && k.0 < upto).cloned());
        }
        for (t, sq) in self.dd.iter().enumerate().take(upto).skip(from) {
            for (i, row) in sq.iter().enumerate() {
                for (j, e) in row.iter().enumerate() {
                    keys.extend(e.keys().map(|k| (t, i, j, k.clone())));
                }
            }
        }
        let rows: Vec<Vec<Q>> = keys
            .iter()
            .map(|k| {
                let mut r: Vec<Q> = self
                    .cols
                    .iter()
                    .map(|c| *c.get(k).unwrap_or(&Q::int(0)))
                    .collect();
                r.push(
                    self.dd[k.0][k.1][k.2]
                        .get(&k.3)
                        .copied()
                        .unwrap_or(Q::int(0))
                        .neg(),
                );
                r
            })
            .collect();
        let a: Vec<Vec<Q>> = rows.iter().map(|r| r[..r.len() - 1].to_vec()).collect();
        if rows.is_empty() {
            return (0, 0);
        }
        (rank(&a), rank(&rows))
    }
    fn consistent(&self) -> bool {
        let (a, b) = self.ranks(self.dd.len());
        a == b
    }
    /// The first square at which the equations of the squares up to it become inconsistent.
    fn first_bad(&self) -> Option<usize> {
        (0..self.dd.len()).find(|&k| {
            let (a, b) = self.ranks(k + 1);
            a != b
        })
    }
    fn kernel_dim(&self) -> usize {
        let (a, _) = self.ranks(self.dd.len());
        self.vars.len() - a
    }
}

#[derive(Clone, Debug)]
struct DeformCase {
    ctx: String,
    n: u32,
    hdeg: i64,
    cx: Cx,
    unknowns: BTreeSet<(usize, usize, usize)>,
}

/// A family of contexts and the objects to build complexes from.
struct Family {
    ctx: &'static str,
    hdeg: i64,
    objects: &'static [&'static str],
    shifts: &'static [i64],
}

const DEFORM_FAMILIES: &[Family] = &[
    // A1, one black strand: e A e = k[x] or k[x]/(x²); h only in the maps
    Family {
        ctx: "klrw type=A1 ring=QQ red=\"2i@0\"",
        hdeg: 2,
        objects: &["i@1"],
        shifts: &[0, 2, 4],
    },
    Family {
        ctx: "klrw type=A1 ring=QQ red=\"2i@0\"",
        hdeg: 0,
        objects: &["i@1"],
        shifts: &[0, 2, 4],
    },
    Family {
        ctx: "klrw type=A1 ring=QQ red=\"2i@0\" quotient=cyclotomic",
        hdeg: 2,
        objects: &["i@1"],
        shifts: &[0, 2, 4],
    },
    Family {
        ctx: "klrw type=A1 ring=QQ red=\"2i@0\" quotient=cyclotomic",
        hdeg: 0,
        objects: &["i@1"],
        shifts: &[0, 2],
    },
    // A2: h in the relations (ψψ e(ij) = h e(ij)) and in the maps
    Family {
        ctx: "klrw type=A2 ring=QQ Q_ij=h",
        hdeg: 2,
        objects: &["i@0 j@1", "j@0 i@1"],
        shifts: &[0, 1, 2, 3],
    },
    Family {
        ctx: "klrw type=A2 ring=QQ Q_ij=\"u + h v\"",
        hdeg: 0,
        objects: &["i@0 j@1", "j@0 i@1"],
        shifts: &[0, 1, 2, 3],
    },
];

fn deform_case(seed: u64, n: u32) -> DeformCase {
    let mut r = Rng(seed | 1);
    let f = &DEFORM_FAMILIES[r.below(DEFORM_FAMILIES.len())];
    let ctx = format!("{} truncate=h^{n} hdeg={}", f.ctx, f.hdeg);
    let mut o = Oracle::new(&ctx, n);
    let nterms = if r.chance(1, 4) { 4 } else { 3 };
    let mut terms: Vec<Vec<(String, i64)>> = (0..nterms)
        .map(|_| {
            let k = 1 + r.below(2);
            (0..k)
                .map(|_| (r.pick(f.objects).to_string(), *r.pick(f.shifts)))
                .collect()
        })
        .collect();
    // shifts decrease along the complex, so that maps of nonnegative degree exist
    for t in (0..nterms - 1).rev() {
        let next_max = terms[t + 1].iter().map(|s| s.1).max().unwrap();
        for s in &mut terms[t] {
            s.1 += next_max;
        }
    }
    let mut cx = Cx {
        diffs: (0..nterms - 1)
            .map(|t| vec![vec![String::new(); terms[t].len()]; terms[t + 1].len()])
            .collect(),
        terms,
    };
    let style = r.below(3);
    for t in 0..nterms - 1 {
        // style 1: the last differential is zero mod h, so d² vanishes at order 0
        let zero_mod_h = style == 1 && t == nterms - 2;
        for i in 0..cx.terms[t + 1].len() {
            for j in 0..cx.terms[t].len() {
                let deg = cx.degree(t, i, j);
                let (src, tgt) = (cx.terms[t][j].0.clone(), cx.terms[t + 1][i].0.clone());
                let free = if zero_mod_h {
                    vec![]
                } else {
                    o.hom_free(&src, &tgt, deg)
                };
                let hpart = o.hom_free(&src, &tgt, deg - f.hdeg);
                let h: Vec<String> = hpart.iter().map(|y| format!("h {y}")).collect();
                let e = add_terms(&combination(&mut r, &free, 2), &combination(&mut r, &h, 1));
                cx.diffs[t][i][j] = e;
            }
        }
    }
    // style 2: a Koszul square P⟨a+b⟩ → P⟨a⟩ ⊕ P⟨b⟩ → P on the last three terms, (p, q)ᵀ then
    // (q, −p), plus a ψψ bigon summand in A2; its h-free part squares to zero
    if style == 2 {
        let obj = f.objects[0].to_string();
        let (a, b) = (*r.pick(&[0i64, 2]), *r.pick(&[0i64, 2]));
        let mut mid = vec![(obj.clone(), a), (obj.clone(), b)];
        let bigon = f.objects.len() > 1 && r.chance(3, 4);
        if bigon {
            mid.push((f.objects[1].to_string(), a + b - 1));
        }
        let k = cx.terms.len();
        cx.terms[k - 3] = vec![(obj.clone(), a + b)];
        cx.terms[k - 2] = mid;
        cx.terms[k - 1] = vec![(obj.clone(), 0)];
        let p = combination(&mut r, &o.hom_free(&obj, &obj, b), 3);
        let q = combination(&mut r, &o.hom_free(&obj, &obj, a), 3);
        let neg = |s: &str| {
            if s.is_empty() {
                String::new()
            } else {
                format!("-({s})")
            }
        };
        let mut d1 = vec![vec![p.clone()], vec![q.clone()]];
        let mut d2 = vec![vec![q, neg(&p)]];
        if bigon {
            let other = f.objects[1];
            let up = combination(&mut r, &o.hom_free(&obj, other, 1), 4);
            let down = combination(&mut r, &o.hom_free(other, &obj, a + b - 1), 4);
            d1.push(vec![up]);
            d2[0].push(down);
        }
        // perturb by multiples of h
        for (t, m) in [(k - 3, &mut d1), (k - 2, &mut d2)] {
            for (i, row) in m.iter_mut().enumerate() {
                for (j, e) in row.iter_mut().enumerate() {
                    let src = cx.terms[t][j].clone();
                    let tgt = cx.terms[t + 1][i].clone();
                    let deg = src.1 - tgt.1 - f.hdeg;
                    let hb: Vec<String> = o
                        .hom_free(&src.0, &tgt.0, deg)
                        .iter()
                        .map(|y| format!("h {y}"))
                        .collect();
                    *e = add_terms(e, &combination(&mut r, &hb, 1));
                }
            }
        }
        cx.diffs[k - 3] = d1;
        cx.diffs[k - 2] = d2;
        if k == 4 {
            // the first differential into the new middle term: zero mod h
            cx.diffs[0] = vec![vec![String::new(); cx.terms[0].len()]; 1];
            let first = cx.terms[0].iter().map(|s| s.1).max().unwrap();
            let need = a + b;
            for s in &mut cx.terms[0] {
                s.1 += (need - first).max(0);
            }
        }
    }
    let mut unknowns = BTreeSet::new();
    for (t, m) in cx.diffs.iter().enumerate() {
        for i in 0..m.len() {
            for j in 0..m[i].len() {
                if r.chance(1, 2) {
                    unknowns.insert((t, i, j));
                }
            }
        }
    }
    DeformCase {
        ctx,
        n,
        hdeg: f.hdeg,
        cx,
        unknowns,
    }
}

fn diff_ref(lo: i64, t: usize) -> Value {
    json!({ "map": "differential", "from": lo + t as i64, "axis": 0 })
}

/// Runs `complex.deform` on the case and checks every field of the output against the oracle.
/// Returns (the order-0 square was a complex, the solver reached the top order).
fn check_deform(case: &DeformCase) -> (bool, bool) {
    let DeformCase {
        ctx,
        n,
        hdeg,
        cx,
        unknowns,
    } = case;
    let lo = cx.lo();
    let req_unknowns: Vec<Value> = unknowns
        .iter()
        .map(|&(t, i, j)| json!({ "map": "differential", "from": lo + t as i64, "row": i, "col": j }))
        .collect();
    let v = call(
        "complex.deform",
        json!({ "context": ctx, "complex": cx.json(), "unknowns": req_unknowns }),
    );
    let mut o = Oracle::new(ctx, *n);
    let ctxt = format!("{case:?}\n{v:#}");
    // the obstruction note appears exactly when there is a failure, with its order
    let notes = v["provenance"]["notes"].to_string();
    assert!(notes.contains(&format!("1 ≤ m ≤ {n}")), "{ctxt}");
    match v["failures"][0]["order"].as_u64() {
        Some(m) => assert!(
            notes.contains(&format!(
                "obstructed at order {m}: the complex returned is corrected through order {}",
                m.saturating_sub(1)
            )),
            "{ctxt}"
        ),
        None => assert!(!notes.contains("obstructed"), "{ctxt}"),
    }
    // order 0
    let s0 = order_system(&mut o, cx, unknowns, 0, *hdeg);
    let bad0 = s0
        .dd
        .iter()
        .position(|sq| sq.iter().flatten().any(|e| !e.is_empty()));
    if let Some(t) = bad0 {
        assert_eq!(v["solved"], false, "{ctxt}");
        assert_eq!(v["solved_to_order"], Value::Null, "{ctxt}");
        assert_eq!(v["orders"], json!([]), "{ctxt}");
        let f = &v["failures"][0];
        assert_eq!(f["order"], 0, "{ctxt}");
        assert_eq!(f["condition"], "d^2", "{ctxt}");
        assert_eq!(
            f["maps"],
            json!([diff_ref(lo, t), diff_ref(lo, t + 1)]),
            "{ctxt}"
        );
        assert_eq!(v["failures"].as_array().unwrap().len(), 1, "{ctxt}");
        return (false, false);
    }
    // orders 1..: rebuild each system from the input and the corrections reported below it
    let orders = v["orders"].as_array().unwrap();
    let mut cur = cx.clone();
    for m in 1..=*n {
        let sys = order_system(&mut o, &cur, unknowns, m, *hdeg);
        assert!(
            sys.lower.is_empty(),
            "lower orders of d² do not vanish at order {m}: {ctxt}"
        );
        let reported = orders.iter().find(|x| x["order"] == m);
        if !sys.consistent() {
            assert!(reported.is_none(), "order {m} is not solvable: {ctxt}");
            assert_eq!(v["solved"], false, "{ctxt}");
            assert_eq!(v["solved_to_order"], m - 1, "{ctxt}");
            let f = &v["failures"][0];
            assert_eq!(f["order"], m, "{ctxt}");
            assert_eq!(f["condition"], "obstruction", "{ctxt}");
            let t = sys.first_bad().expect("an inconsistent prefix");
            assert_eq!(
                f["maps"],
                json!([diff_ref(lo, t), diff_ref(lo, t + 1)]),
                "{ctxt}"
            );
            // the printed entry is the first nonzero entry of [d²]_m at that square, or 0
            let detail = f["detail"].as_str().unwrap();
            let entry = detail
                .split_once("has the entry ")
                .and_then(|(_, r)| r.split_once(" here;").or_else(|| r.split_once("; it is")))
                .map(|(e, _)| e)
                .unwrap_or_else(|| panic!("no entry in {detail:?}"));
            let first = sys.dd[t].iter().flatten().find(|e| !e.is_empty());
            let printed = o.nf(entry);
            assert!(printed.keys().all(|(h, _)| *h == 0), "{ctxt}");
            match first {
                Some(e) => assert_eq!(&part(&printed, 0), e, "{ctxt}"),
                None => assert!(printed.is_empty(), "{ctxt}"),
            }
            // the square alone is solvable exactly when the detail says so
            let (a1, b1) = sys.ranks_in(t, t + 1);
            assert_eq!(
                detail.contains("can be met on its own"),
                a1 == b1,
                "alone: {ctxt}"
            );
            assert_eq!(
                detail.contains("it is not of the form"),
                a1 != b1,
                "not alone: {ctxt}"
            );
            // the greedy remark appears exactly when a lower order had freedom
            let lower_free = orders.iter().any(|x| {
                x["order"].as_u64().unwrap() < u64::from(m)
                    && x["freedom_dimension"].as_u64().unwrap() > 0
            });
            assert_eq!(
                detail.contains("a different choice"),
                lower_free,
                "greedy: {ctxt}"
            );
            assert!(detail.contains(&format!("h^{m}")), "{ctxt}");
            same_complex(&mut o, &cur, &v["complex"], &ctxt);
            return (true, false);
        }
        let x =
            reported.unwrap_or_else(|| panic!("order {m} is solvable but not reported: {ctxt}"));
        assert_eq!(
            x["freedom_dimension"],
            sys.kernel_dim(),
            "order {m}: {ctxt}"
        );
        // the correction: only unknown entries, multiples of h^m, and it solves the system
        let mut next = cur.clone();
        for d in x["correction"].as_array().unwrap() {
            let t = (d["at"].as_i64().unwrap() - lo) as usize;
            for (i, row) in d["matrix"].as_array().unwrap().iter().enumerate() {
                for (j, e) in row.as_array().unwrap().iter().enumerate() {
                    let Some(e) = e.as_str() else { continue };
                    assert!(
                        unknowns.contains(&(t, i, j)),
                        "correction at a known entry: {ctxt}"
                    );
                    let l = o.nf(e);
                    assert!(
                        l.keys().all(|(h, _)| *h == m),
                        "correction not h^{m} X: {ctxt}"
                    );
                    next.diffs[t][i][j] = add_terms(&next.diffs[t][i][j], e);
                }
            }
        }
        let after = order_system(&mut o, &next, unknowns, m, *hdeg);
        assert!(after.lower.is_empty(), "{ctxt}");
        assert!(
            after.dd.iter().flatten().flatten().all(BTreeMap::is_empty),
            "the correction at order {m} does not solve it: {ctxt}"
        );
        // the freedom: independent solutions of the homogeneous system
        let free = x["freedom"].as_array().unwrap();
        assert_eq!(free.len(), sys.kernel_dim().min(20), "{ctxt}");
        let cap = format!("order {m}: the freedom has dimension");
        assert_eq!(
            v["provenance"]["notes"].to_string().contains(&cap),
            sys.kernel_dim() > 20,
            "freedom cap note: {ctxt}"
        );
        for y in free {
            let mut plus = cur.clone();
            for d in y.as_array().unwrap() {
                let t = (d["at"].as_i64().unwrap() - lo) as usize;
                for (i, row) in d["matrix"].as_array().unwrap().iter().enumerate() {
                    for (j, e) in row.as_array().unwrap().iter().enumerate() {
                        let Some(e) = e.as_str() else { continue };
                        assert!(
                            unknowns.contains(&(t, i, j)),
                            "freedom at a known entry: {ctxt}"
                        );
                        let l = o.nf(e);
                        assert!(l.keys().all(|(h, _)| *h == m), "{ctxt}");
                        plus.diffs[t][i][j] = add_terms(&plus.diffs[t][i][j], e);
                    }
                }
            }
            // Y solves the homogeneous system: adding it to the uncorrected complex leaves the
            // h^m coefficient of d² unchanged
            let with = order_system(&mut o, &plus, unknowns, m, *hdeg);
            assert_eq!(
                with.dd, sys.dd,
                "a freedom vector is not in the kernel at order {m}: {ctxt}"
            );
        }
        // independence of the freedom vectors, on the union of their coordinates
        let mut all_keys: BTreeSet<(usize, usize, usize, String)> = BTreeSet::new();
        let mut maps: Vec<BTreeMap<(usize, usize, usize, String), Q>> = Vec::new();
        for y in free {
            let mut mp = BTreeMap::new();
            for d in y.as_array().unwrap() {
                let t = (d["at"].as_i64().unwrap() - lo) as usize;
                for (i, row) in d["matrix"].as_array().unwrap().iter().enumerate() {
                    for (j, e) in row.as_array().unwrap().iter().enumerate() {
                        if let Some(e) = e.as_str() {
                            for ((_, k), c) in o.nf(e) {
                                all_keys.insert((t, i, j, k.clone()));
                                mp.insert((t, i, j, k), c);
                            }
                        }
                    }
                }
            }
            maps.push(mp);
        }
        let rows: Vec<Vec<Q>> = maps
            .iter()
            .map(|mp| {
                all_keys
                    .iter()
                    .map(|k| *mp.get(k).unwrap_or(&Q::int(0)))
                    .collect()
            })
            .collect();
        assert_eq!(rank(&rows), free.len(), "dependent freedom vectors: {ctxt}");
        cur = next;
    }
    assert_eq!(v["solved"], true, "{ctxt}");
    assert_eq!(v["solved_to_order"], *n, "{ctxt}");
    assert_eq!(v["failures"], json!([]), "{ctxt}");
    same_complex(&mut o, &cur, &v["complex"], &ctxt);
    (true, true)
}

/// The returned complex is `expected`, entry by entry as straightened terms.
fn same_complex(o: &mut Oracle, expected: &Cx, got: &Value, ctxt: &str) {
    let g = expected.with_diffs_from(got);
    for (t, m) in g.diffs.iter().enumerate() {
        for (i, row) in m.iter().enumerate() {
            for (j, e) in row.iter().enumerate() {
                let d = lin_sub(&o.nf(e), &o.nf(&expected.diffs[t][i][j]));
                assert!(
                    d.is_empty(),
                    "returned entry ({t}, {i}, {j}) differs: {ctxt}"
                );
            }
        }
    }
}

/// The returned complex agrees with input + corrections, and squares to zero mod h^{N+1} when solved.
fn check_returned_complex(case: &DeformCase, v: &Value, solved: bool) {
    let mut o = Oracle::new(&case.ctx, case.n);
    let out = case.cx.with_diffs_from(&v["complex"]);
    for (t, m) in out.diffs.iter().enumerate() {
        for (i, row) in m.iter().enumerate() {
            for (j, e) in row.iter().enumerate() {
                let diff = lin_sub(&o.nf(e), &o.nf(&case.cx.diffs[t][i][j]));
                if case.unknowns.contains(&(t, i, j)) {
                    assert!(
                        diff.keys().all(|(h, _)| *h >= 1),
                        "the h-free part changed: {v:#}"
                    );
                } else {
                    assert!(
                        diff.is_empty(),
                        "a known entry changed at ({t}, {i}, {j}): {v:#}"
                    );
                }
            }
        }
    }
    if solved {
        let s = order_system(&mut o, &out, &BTreeSet::new(), case.n, case.hdeg);
        assert!(
            s.lower.is_empty(),
            "the returned complex is not one below h^{}: {v:#}",
            case.n
        );
        assert!(
            s.dd.iter().flatten().flatten().all(BTreeMap::is_empty),
            "{v:#}"
        );
        let c = call(
            "complex.check",
            json!({ "context": case.ctx, "complex": v["complex"] }),
        );
        assert_eq!(c["holds"], true, "{c:#}");
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 48, failure_persistence: None, rng_seed: FIXED_SEED, ..ProptestConfig::default() })]

    /// Order 1 is a linear system, so the oracle decides it exactly.
    #[test]
    fn deform_agrees_with_brute_force_at_order_one(seed in any::<u64>()) {
        let case = deform_case(seed, 1);
        check_deform(&case);
        let v = call("complex.deform", json!({
            "context": case.ctx,
            "complex": case.cx.json(),
            "unknowns": case.unknowns.iter().map(|&(t, i, j)| json!({ "map": "differential", "from": case.cx.lo() + t as i64, "row": i, "col": j })).collect::<Vec<_>>(),
        }));
        check_returned_complex(&case, &v, v["solved"] == true);
    }

    /// Order 2 along the solver's own choice at order 1.
    #[test]
    fn deform_agrees_with_brute_force_at_order_two(seed in any::<u64>()) {
        let case = deform_case(seed, 2);
        check_deform(&case);
        let v = call("complex.deform", json!({
            "context": case.ctx,
            "complex": case.cx.json(),
            "unknowns": case.unknowns.iter().map(|&(t, i, j)| json!({ "map": "differential", "from": case.cx.lo() + t as i64, "row": i, "col": j })).collect::<Vec<_>>(),
        }));
        check_returned_complex(&case, &v, v["solved"] == true);
    }
}

/// A fixed sweep of 300 cases at order 1, checked like the random ones, which also asserts that
/// the generator reaches every branch: order-0 failures, obstructions and solutions, in every
/// family, and A2 middle terms with a ψψ bigon (where `h` arises only from a composite) that
/// pass order 0. The random tests above draw too few of the latter to be relied on alone.
#[test]
fn deform_sweep_covers_every_outcome() {
    let mut seen = BTreeSet::new();
    let mut bigons = 0;
    for seed in 1..300u64 {
        let case = deform_case(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15), 1);
        let (zero_ok, solved) = check_deform(&case);
        let family = case.ctx.split(" truncate").next().unwrap().to_string();
        seen.insert((family.clone(), zero_ok, solved));
        if family.contains("A2") && zero_ok && case.cx.terms.iter().any(|t| t.len() == 3) {
            bigons += 1;
        }
    }
    for f in DEFORM_FAMILIES {
        for outcome in [(false, false), (true, false), (true, true)] {
            assert!(
                seen.iter()
                    .any(|(g, a, b)| g == f.ctx && (*a, *b) == outcome),
                "no case of {outcome:?} in {}: {seen:?}",
                f.ctx
            );
        }
    }
    assert!(bigons >= 10, "only {bigons} bigon cases passed order 0");
}

// ---------------------------------------------------------------------------------------------
// module.head

/// Coordinates of `v` on `basis` (exact), or `None` if `v` is not in its span.
fn coords(basis: &[Lin], v: &Lin) -> Option<Vec<Q>> {
    let keys: Vec<(u32, String)> = basis
        .iter()
        .chain(std::iter::once(v))
        .flat_map(|l| l.keys().cloned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let n = basis.len();
    // rows: one per key, columns: the basis vectors and then v
    let mut m: Vec<Vec<Q>> = keys
        .iter()
        .map(|k| {
            let mut row: Vec<Q> = basis
                .iter()
                .map(|b| *b.get(k).unwrap_or(&Q::int(0)))
                .collect();
            row.push(*v.get(k).unwrap_or(&Q::int(0)));
            row
        })
        .collect();
    let mut pivots = Vec::new();
    let mut r = 0;
    for c in 0..n {
        let Some(p) = (r..m.len()).find(|&i| !m[i][c].zero()) else {
            continue;
        };
        m.swap(r, p);
        let inv = Q::int(1).div(m[r][c]);
        for k in 0..=n {
            m[r][k] = m[r][k].mul(inv);
        }
        for i in 0..m.len() {
            if i != r && !m[i][c].zero() {
                let f = m[i][c];
                for k in 0..=n {
                    let s = m[r][k].mul(f);
                    m[i][k] = m[i][k].sub(s);
                }
            }
        }
        pivots.push(c);
        r += 1;
    }
    assert_eq!(r, n, "the basis is not linearly independent");
    if m[r..].iter().any(|row| !row[n].zero()) {
        return None;
    }
    let mut x = vec![Q::int(0); n];
    for (i, &c) in pivots.iter().enumerate() {
        x[c] = m[i][n];
    }
    Some(x)
}

/// `End⁰(o)`: its basis, the trace functional `τ(a) = tr(b ↦ a ∘ b)`, and `dim End⁰/J`, the
/// rank of the trace form `(a, b) ↦ τ(a ∘ b)` (over `ℚ` its radical is `J`).
struct EndZero {
    basis: Vec<Lin>,
    tau: Vec<Q>,
    simple_dim: usize,
}

impl EndZero {
    fn new(o: &mut Oracle, obj: &str) -> EndZero {
        let names = o.hom(obj, obj, 0);
        let basis: Vec<Lin> = names.iter().map(|b| o.nf(b)).collect();
        let n = basis.len();
        // mult[a][b] = coordinates of b_a ∘ b_b
        let mult: Vec<Vec<Vec<Q>>> = (0..n)
            .map(|a| {
                (0..n)
                    .map(|b| {
                        let p = o.sum_of_products(&[(&names[a], &names[b])]);
                        coords(&basis, &p)
                            .unwrap_or_else(|| panic!("End⁰({obj}) is not closed: {p:?}"))
                    })
                    .collect()
            })
            .collect();
        let tau: Vec<Q> = (0..n)
            .map(|a| (0..n).fold(Q::int(0), |s, k| s.add(mult[a][k][k])))
            .collect();
        let form: Vec<Vec<Q>> = (0..n)
            .map(|a| (0..n).map(|b| dot(&tau, &mult[a][b])).collect())
            .collect();
        EndZero {
            basis,
            tau,
            simple_dim: rank(&form),
        }
    }

    /// `τ(c)` for an endomorphism given as coordinates-to-be on the basis.
    fn trace(&self, c: &Lin) -> Q {
        dot(
            &self.tau,
            &coords(&self.basis, c).expect("not an endomorphism of degree 0"),
        )
    }
}

fn dot(a: &[Q], b: &[Q]) -> Q {
    a.iter()
        .zip(b)
        .fold(Q::int(0), |s, (x, y)| s.add(x.mul(*y)))
}

struct HeadFamily {
    ctx: &'static str,
    /// Objects with the same strands, among which summands and tests are drawn.
    classes: &'static [&'static [&'static str]],
}

const HEAD_FAMILIES: &[HeadFamily] = &[
    HeadFamily {
        ctx: "klrw type=A1 ring=QQ red=\"2i@0\"",
        classes: &[&["i@1"], &["i@1 i@2", "i@-1/2 i@1/2"]],
    },
    HeadFamily {
        ctx: "klrw type=A1 ring=QQ red=\"2i@0\" quotient=cyclotomic",
        classes: &[&["i@1"], &["i@1 i@2", "i@-1/2 i@1/2"]],
    },
    HeadFamily {
        ctx: "klrw type=A1 ring=QQ red=\"2i@0\" truncate=h^1 hdeg=2",
        classes: &[&["i@1"], &["i@1 i@2", "i@-1/2 i@1/2"]],
    },
    HeadFamily {
        ctx: "klrw type=A1 ring=QQ red=\"2i@0\" quotient=cyclotomic truncate=h^2 hdeg=0",
        classes: &[&["i@1"], &["i@1 i@2", "i@-1/2 i@1/2"]],
    },
    HeadFamily {
        ctx: "klrw type=A2 ring=QQ",
        classes: &[&["i@0 j@1", "j@0 i@1"]],
    },
    HeadFamily {
        ctx: "klrw type=A2 ring=QQ Q_ij=h truncate=h^1 hdeg=2",
        classes: &[&["i@0 j@1", "j@0 i@1"]],
    },
    HeadFamily {
        ctx: "klrw type=A2 ring=QQ Q_ij=\"u + h v\" truncate=h^1 hdeg=0",
        classes: &[&["i@0 j@1", "j@0 i@1"]],
    },
];

#[derive(Debug)]
struct HeadCase {
    ctx: String,
    summands: Vec<(String, i64)>,
    class: Vec<String>,
    max_degree: Option<i64>,
}

fn head_case(seed: u64) -> HeadCase {
    let mut r = Rng(seed | 1);
    let f = r.pick(HEAD_FAMILIES);
    let class = *r.pick(f.classes);
    let k = 1 + r.below(3);
    let summands = (0..k)
        .map(|_| (r.pick(class).to_string(), *r.pick(&[0i64, 1, 2, -2])))
        .collect();
    let max_degree = *r.pick(&[None, Some(2), Some(4)]);
    HeadCase {
        ctx: f.ctx.to_string(),
        summands,
        class: class.iter().map(|s| s.to_string()).collect(),
        max_degree,
    }
}

/// The truncation order of a context (0 if there is none).
fn order_of(ctx: &str) -> u32 {
    ctx.split_once("truncate=h^").map_or(0, |(_, rest)| {
        rest.split(' ').next().unwrap().parse().unwrap()
    })
}

/// `{(test, degree) ↦ dim}` from a `dimensions` array, without zeros.
fn read_dims(v: &Value) -> BTreeMap<(String, i64), i64> {
    let mut out = BTreeMap::new();
    for t in v.as_array().unwrap() {
        for p in t["powers"].as_array().unwrap() {
            let (d, n) = (p[0].as_i64().unwrap(), p[1].as_i64().unwrap());
            assert!(n > 0, "a zero entry in {v}");
            out.insert((t["test"].as_str().unwrap().to_string(), d), n);
        }
    }
    out
}

/// Runs `module.head` and checks it against the oracle. Returns whether the simplicity note
/// fired and whether the window was truncated.
fn check_head(case: &HeadCase) -> (bool, bool) {
    let mut req = json!({
        "context": case.ctx,
        "summands": case.summands.iter().map(|(o, s)| json!({ "object": o, "shift": s })).collect::<Vec<_>>(),
    });
    if let Some(m) = case.max_degree {
        req["max_degree"] = json!(m);
    }
    let v = call("module.head", req);
    let ctxt = format!("{case:?}\n{v:#}");
    let mut o = Oracle::new(&case.ctx, order_of(&case.ctx));
    let mut ends: BTreeMap<String, EndZero> = BTreeMap::new();
    for (obj, _) in &case.summands {
        if !ends.contains_key(obj) {
            ends.insert(obj.clone(), EndZero::new(&mut o, obj));
        }
    }
    // the simplicity note, exactly for the objects whose End⁰/J is not one-dimensional
    let notes: Vec<String> = v["provenance"]["notes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_str().unwrap().to_string())
        .collect();
    let mut fired = false;
    for (obj, e) in &ends {
        let text = format!("End⁰({obj})/J has dimension {}, so", e.simple_dim);
        let named = notes
            .iter()
            .filter(|n| n.contains(&format!("End⁰({obj})/J")))
            .count();
        if e.simple_dim > 1 {
            assert!(
                notes.iter().any(|n| n.contains(&text)),
                "missing note `{text}`: {ctxt}"
            );
            assert_eq!(named, 1, "{ctxt}");
            fired = true;
        } else {
            assert_eq!(named, 0, "spurious simplicity note for {obj}: {ctxt}");
        }
    }
    let simplicity = notes
        .iter()
        .filter(|n| n.contains("may not be simple"))
        .count();
    assert_eq!(
        simplicity,
        ends.values().filter(|e| e.simple_dim > 1).count(),
        "{ctxt}"
    );
    // the window and its note
    // no window means no maps from any test object at all, so the head is zero; the oracle then
    // looks at a range of degrees around zero
    let w = &v["window"];
    let (from, to, truncated) = if w.is_null() {
        (-4, 4, false)
    } else {
        (
            w["from"].as_i64().unwrap(),
            w["to"].as_i64().unwrap(),
            w["truncated"] == true,
        )
    };
    assert_eq!(v["complete"], !truncated, "{ctxt}");
    let window_note = notes
        .iter()
        .any(|n| n.contains("the radical was searched for relations in degrees"));
    assert_eq!(window_note, truncated, "{ctxt}");
    if truncated {
        assert!(
            notes
                .iter()
                .any(|n| n.contains(&format!("in degrees {from} to {to} only"))),
            "{ctxt}"
        );
    }
    if w.is_null() {
        assert_eq!(v["dimension"], 0, "{ctxt}");
    }
    // the dimensions of the head, test object by test object, degree by degree
    let got = read_dims(&v["dimensions"]);
    assert_eq!(
        v["dimension"].as_i64().unwrap(),
        got.values().sum::<i64>(),
        "{ctxt}"
    );
    let mut tests: BTreeSet<String> = case.class.iter().cloned().collect();
    tests.extend(got.keys().map(|(t, _)| t.clone()));
    let mut expected = BTreeMap::new();
    for t in &tests {
        // below the window, and above it when it is not truncated, the Hom spaces vanish
        for d in from - 2..=if truncated { to } else { to + 2 } {
            let mut dim = 0;
            for (obj, s) in &case.summands {
                let e = &ends[obj];
                let fs = o.hom(t, obj, d - s);
                let gs = o.hom(obj, t, s - d);
                let m: Vec<Vec<Q>> = fs
                    .iter()
                    .map(|f| {
                        gs.iter()
                            .map(|g| e.trace(&o.sum_of_products(&[(f, g)])))
                            .collect()
                    })
                    .collect();
                dim += rank(&m) as i64;
            }
            if dim > 0 {
                expected.insert((t.clone(), d), dim);
            }
        }
    }
    let in_window: BTreeMap<_, _> = got
        .iter()
        .filter(|((_, d), _)| (from..=to).contains(d))
        .map(|(k, n)| (k.clone(), *n))
        .collect();
    assert_eq!(in_window, got, "dimensions outside the window: {ctxt}");
    assert_eq!(got, expected, "head dimensions: {ctxt}");
    // the relations lie in the radical, and generate it in the window
    let module = &v["module"];
    let gens = module["generators"].as_array().unwrap();
    let rels = module["relations"].as_array().unwrap();
    for (j, rel) in rels.iter().enumerate() {
        let (r_obj, r_shift) = (
            rel["object"].as_str().unwrap(),
            rel["shift"].as_i64().unwrap(),
        );
        for (i, g) in gens.iter().enumerate() {
            let entry = &module["matrix"][i][j];
            let Some(f) = entry.as_str() else { continue };
            let (obj, s) = (g["object"].as_str().unwrap(), g["shift"].as_i64().unwrap());
            let d = r_shift - s;
            let basis: Vec<Lin> = o.hom(r_obj, obj, d).iter().map(|b| o.nf(b)).collect();
            assert!(
                coords(&basis, &o.nf(f)).is_some(),
                "relation {j} is not of degree {d}: {ctxt}"
            );
            for back in o.hom(obj, r_obj, -d) {
                let c = o.sum_of_products(&[(f, &back)]);
                assert!(
                    ends[obj].trace(&c).zero(),
                    "relation {j} is not in the radical: {ctxt}"
                );
            }
        }
    }
    // the presented module has the dimensions of the head: at each test object and degree,
    // Hom(t, ⊕ generators) modulo the images of Hom(t, relation) under the matrix, computed here
    // from hom.basis and simplify only
    let mut presented = BTreeMap::new();
    for t in &tests {
        for d in from..=to {
            let blocks: Vec<Vec<Lin>> = gens
                .iter()
                .map(|g| {
                    let (obj, s) = (g["object"].as_str().unwrap(), g["shift"].as_i64().unwrap());
                    o.hom(t, obj, d - s).iter().map(|b| o.nf(b)).collect()
                })
                .collect();
            let total: usize = blocks.iter().map(Vec::len).sum();
            let mut image: Vec<Vec<Q>> = Vec::new();
            for (j, rel) in rels.iter().enumerate() {
                let (r_obj, r_shift) = (
                    rel["object"].as_str().unwrap(),
                    rel["shift"].as_i64().unwrap(),
                );
                for b in o.hom(t, r_obj, d - r_shift) {
                    let mut row = Vec::with_capacity(total);
                    for (i, basis) in blocks.iter().enumerate() {
                        let comp = match module["matrix"][i][j].as_str() {
                            Some(f) => o.sum_of_products(&[(f, &b)]),
                            None => Lin::new(),
                        };
                        row.extend(coords(basis, &comp).unwrap_or_else(|| {
                            panic!("relation {j} composed into generator {i}: {ctxt}")
                        }));
                    }
                    image.push(row);
                }
            }
            let dim = total as i64 - rank(&image) as i64;
            if dim > 0 {
                presented.insert((t.clone(), d), dim);
            }
        }
    }
    let expected_in_window: BTreeMap<_, _> = expected
        .iter()
        .filter(|((_, d), _)| (from..=to).contains(d))
        .map(|(k, n)| (k.clone(), *n))
        .collect();
    assert_eq!(
        presented, expected_in_window,
        "the presented module is not the head: {ctxt}"
    );
    (fired, truncated)
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 32, failure_persistence: None, rng_seed: FIXED_SEED, ..ProptestConfig::default() })]

    #[test]
    fn head_agrees_with_brute_force(seed in any::<u64>()) {
        check_head(&head_case(seed));
    }
}

/// A fixed sweep, which also asserts that the simplicity note and the window note each fire
/// and stay silent somewhere, in contexts with and without `h`.
#[test]
fn head_sweep_covers_every_diagnostic() {
    let mut seen = BTreeSet::new();
    for seed in 1..120u64 {
        let case = head_case(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let (fired, truncated) = check_head(&case);
        seen.insert((case.ctx.contains('h'), fired, truncated));
    }
    for with_h in [false, true] {
        for fired in [false, true] {
            assert!(
                seen.iter().any(|s| s.0 == with_h && s.1 == fired),
                "{seen:?}"
            );
        }
    }
    assert!(
        seen.iter().any(|s| s.2) && seen.iter().any(|s| !s.2),
        "{seen:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// complexes, chain maps, homotopies, modules, and resolutions
//
// The oracle writes every element of `Hom(T⟨d⟩, ⊕ X_j⟨s_j⟩)` as a column of terms from
// `hom.basis`, composes by writing `(a)(b)` and straightening with `simplify`, and does the linear
// algebra here: homology as kernel modulo image, kernels and their generation, the radical through
// the trace form of `End⁰`, the module `coker φ`. Test objects are the objects of the family
// together with those an operation reports.

/// A list of summands `X⟨s⟩`.
type Sums = Vec<(String, i64)>;
/// A matrix of terms (rows: target summands, columns: source summands; "" is zero).
type Mat = Vec<Vec<String>>;
/// An element of a sum as coordinates: (summand, power of `h`, monomial).
type Elt = BTreeMap<(usize, u32, String), Q>;

/// The rank of a list of elements.
fn rank_of(vs: &[Elt]) -> usize {
    let keys: Vec<_> = vs
        .iter()
        .flat_map(|v| v.keys().cloned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let rows: Vec<Vec<Q>> = vs
        .iter()
        .map(|v| {
            keys.iter()
                .map(|k| *v.get(k).unwrap_or(&Q::int(0)))
                .collect()
        })
        .collect();
    rank(&rows)
}

/// A basis of the combinations `c` with `Σ c_k vs[k] = 0`, scaled to integers.
fn relations_among(vs: &[Elt]) -> Vec<Vec<Q>> {
    let keys: Vec<_> = vs
        .iter()
        .flat_map(|v| v.keys().cloned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let n = vs.len();
    let mut m: Vec<Vec<Q>> = keys
        .iter()
        .map(|k| vs.iter().map(|v| *v.get(k).unwrap_or(&Q::int(0))).collect())
        .collect();
    let mut pivots = Vec::new();
    let mut r = 0;
    for c in 0..n {
        let Some(p) = (r..m.len()).find(|&i| !m[i][c].zero()) else {
            continue;
        };
        m.swap(r, p);
        let inv = Q::int(1).div(m[r][c]);
        for k in 0..n {
            m[r][k] = m[r][k].mul(inv);
        }
        for i in 0..m.len() {
            if i != r && !m[i][c].zero() {
                let f = m[i][c];
                for k in 0..n {
                    let s = m[r][k].mul(f);
                    m[i][k] = m[i][k].sub(s);
                }
            }
        }
        pivots.push(c);
        r += 1;
    }
    let mut out = Vec::new();
    for free in (0..n).filter(|c| !pivots.contains(c)) {
        let mut x = vec![Q::int(0); n];
        x[free] = Q::int(1);
        for (i, &c) in pivots.iter().enumerate() {
            x[c] = m[i][free].neg();
        }
        let l = x.iter().fold(1i128, |l, q| l / gcd(l, q.1) * q.1);
        out.push(x.iter().map(|q| Q::int(q.0 * (l / q.1))).collect());
    }
    out
}

fn elt_add(a: &mut Elt, b: &Elt, c: Q) {
    for (k, v) in b {
        let e = a.entry(k.clone()).or_insert(Q::int(0));
        *e = e.add(v.mul(c));
    }
    a.retain(|_, v| !v.zero());
}

/// `c · t` as text.
fn scaled(c: Q, t: &str) -> String {
    if c.zero() || t.is_empty() {
        return String::new();
    }
    let s = if c.1 == 1 {
        format!("{}", c.0)
    } else {
        format!("{}/{}", c.0, c.1)
    };
    format!("({s})({t})")
}

/// `Σ` of texts, skipping zeros.
fn text_sum(ts: impl IntoIterator<Item = String>) -> String {
    let ts: Vec<String> = ts.into_iter().filter(|t| !t.is_empty()).collect();
    ts.join(" + ")
}

/// The product `a ∘ b` of matrices of terms, as text.
fn mat_mul(a: &Mat, b: &Mat) -> Mat {
    let inner = b.len();
    let cols = b.first().map_or(0, Vec::len);
    a.iter()
        .map(|row| {
            (0..cols)
                .map(|k| {
                    text_sum((0..inner).filter_map(|j| {
                        let (x, y) = (&row[j], &b[j][k]);
                        (!x.is_empty() && !y.is_empty()).then(|| format!("({x})({y})"))
                    }))
                })
                .collect()
        })
        .collect()
}

fn mat_add(a: &Mat, b: &Mat) -> Mat {
    a.iter()
        .zip(b)
        .map(|(r, s)| {
            r.iter()
                .zip(s)
                .map(|(x, y)| text_sum([x.clone(), y.clone()]))
                .collect()
        })
        .collect()
}

fn mat_json(m: &Mat) -> Value {
    json!(
        m.iter()
            .map(|r| r
                .iter()
                .map(|f| if f.is_empty() { Value::Null } else { json!(f) })
                .collect::<Vec<_>>())
            .collect::<Vec<_>>()
    )
}

fn mat_read(v: &Value) -> Mat {
    v.as_array()
        .unwrap()
        .iter()
        .map(|r| {
            r.as_array()
                .unwrap()
                .iter()
                .map(|f| f.as_str().unwrap_or("").to_string())
                .collect()
        })
        .collect()
}

fn sums_json(s: &Sums) -> Value {
    json!(
        s.iter()
            .map(|(o, k)| json!({ "object": o, "shift": k }))
            .collect::<Vec<_>>()
    )
}

fn sums_read(v: &Value) -> Sums {
    v.as_array()
        .unwrap()
        .iter()
        .map(|s| {
            (
                s["object"].as_str().unwrap().to_string(),
                s["shift"].as_i64().unwrap(),
            )
        })
        .collect()
}

/// A complex as returned by an operation, positions from its lowest to 0 or its highest.
fn cx_read(v: &Value) -> (i64, Vec<Sums>, Vec<Mat>) {
    let terms = v["terms"].as_array().unwrap();
    let (Some(lo), Some(hi)) = (
        terms.iter().map(|t| t["at"].as_i64().unwrap()).min(),
        terms.iter().map(|t| t["at"].as_i64().unwrap()).max(),
    ) else {
        return (0, vec![], vec![]);
    };
    let mut ts = vec![Sums::new(); (hi - lo + 1) as usize];
    for t in terms {
        ts[(t["at"].as_i64().unwrap() - lo) as usize] = sums_read(&t["summands"]);
    }
    let mut ds: Vec<Mat> = (0..ts.len().saturating_sub(1))
        .map(|t| vec![vec![String::new(); ts[t].len()]; ts[t + 1].len()])
        .collect();
    for d in v["differentials"].as_array().unwrap() {
        ds[(d["from"].as_i64().unwrap() - lo) as usize] = mat_read(&d["matrix"]);
    }
    (lo, ts, ds)
}

impl Oracle {
    /// The basis of `Hom(T⟨d⟩, ⊕ X_j⟨s_j⟩)` as columns.
    fn space(&mut self, t: &str, d: i64, xs: &Sums) -> Vec<Vec<String>> {
        let mut out = Vec::new();
        for (j, (x, s)) in xs.iter().enumerate() {
            for b in self.hom(t, x, d - s) {
                let mut c = vec![String::new(); xs.len()];
                c[j] = b;
                out.push(c);
            }
        }
        out
    }

    /// `m ∘ col` as coordinates (`m` empty for the identity).
    fn apply(&mut self, m: Option<&Mat>, col: &[String]) -> Elt {
        let mut out = Elt::new();
        let rows: Vec<Vec<(String, String)>> = match m {
            None => col
                .iter()
                .map(|f| vec![(String::new(), f.clone())])
                .collect(),
            Some(m) => m
                .iter()
                .map(|row| row.iter().cloned().zip(col.iter().cloned()).collect())
                .collect(),
        };
        for (i, row) in rows.iter().enumerate() {
            let l = if m.is_none() {
                self.nf(&row[0].1)
            } else {
                let pairs: Vec<(&str, &str)> =
                    row.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
                self.sum_of_products(&pairs)
            };
            for ((h, mon), q) in l {
                out.insert((i, h, mon), q);
            }
        }
        out
    }

    /// The degrees in `lo..=hi` with a nonzero Hom space from `a` to `b`.
    fn hom_degrees(&mut self, a: &str, b: &str, lo: i64, hi: i64) -> Vec<i64> {
        (lo..=hi)
            .filter(|&d| !self.hom(a, b, d).is_empty())
            .collect()
    }

    /// The homology at the middle of `X --f--> Y --g--> Z` in `Hom(T⟨d⟩, −)` (`f`, `g` may be
    /// absent): its dimension, with a basis of the cycles and the boundaries as coordinates.
    fn homology_dim(
        &mut self,
        t: &str,
        d: i64,
        prev: Option<(&Sums, &Mat)>,
        here: &Sums,
        next: Option<&Mat>,
    ) -> usize {
        let sp = self.space(t, d, here);
        let cycles = match next {
            None => sp.len(),
            Some(g) => {
                let imgs: Vec<Elt> = sp.iter().map(|c| self.apply(Some(g), c)).collect();
                sp.len() - rank_of(&imgs)
            }
        };
        let bounds = match prev {
            None => 0,
            Some((x, f)) => {
                let spx = self.space(t, d, x);
                let imgs: Vec<Elt> = spx.iter().map(|c| self.apply(Some(f), c)).collect();
                rank_of(&imgs)
            }
        };
        cycles - bounds
    }

    /// Whether `d_ij : X⟨s⟩ → Y⟨t⟩` lies in the radical: `g ∘ d_ij ∈ J(End⁰ X)` for every map
    /// back `g : Y⟨t⟩ → X⟨s⟩`, with `J` the radical of the trace form.
    fn in_radical(
        &mut self,
        ends: &mut BTreeMap<String, EndZero>,
        x: &(String, i64),
        y: &(String, i64),
        f: &str,
    ) -> bool {
        if f.is_empty() || self.nf(f).is_empty() {
            return true;
        }
        if !ends.contains_key(&x.0) {
            let e = EndZero::new(self, &x.0);
            ends.insert(x.0.clone(), e);
        }
        let gs = self.hom(&y.0, &x.0, y.1 - x.1);
        let bs = self.hom(&x.0, &x.0, 0);
        for g in &gs {
            for b in &bs {
                let p = self.nf(&format!("({b})({g})({f})"));
                if !ends[&x.0].trace(&p).zero() {
                    return false;
                }
            }
        }
        true
    }
}

/// A family of contexts for complexes: the objects (of several strand counts) to draw summands
/// from, which also serve as test objects.
struct CxFamily {
    ctx: &'static str,
    objects: &'static [&'static str],
}

const CX_FAMILIES: &[CxFamily] = &[
    // A1 cyclotomic, one red strand; one and two black strands
    CxFamily {
        ctx: "klrw type=A1 ring=QQ red=\"2i@0\" quotient=cyclotomic",
        objects: &["i@1", "i@1 i@2", "i@-1/2 i@1/2"],
    },
    // h of degree 0 in the maps
    CxFamily {
        ctx: "klrw type=A1 ring=QQ red=\"2i@0\" quotient=cyclotomic truncate=h^2 hdeg=0",
        objects: &["i@1", "i@1 i@2"],
    },
    // h of negative degree: the degree sweep finds redundant generators
    CxFamily {
        ctx: "klrw type=A1 ring=QQ red=\"2i@0\" quotient=cyclotomic truncate=h^1 hdeg=-2",
        objects: &["i@1"],
    },
    // A2 with h in the relations (ψψ e(ij) = h e(ij)); infinite-dimensional, so windows cut
    CxFamily {
        ctx: "klrw type=A2 ring=QQ Q_ij=h truncate=h^1 hdeg=2",
        objects: &["i@0 j@1", "j@0 i@1"],
    },
    // B2 with two red strands, one and two black strands
    CxFamily {
        ctx: "klrw type=B2 ring=QQ red=\"i@0, j@1\" quotient=cyclotomic",
        objects: &["i@1/2", "i@2", "i@1/2 j@3/2", "j@3/2 i@2", "i@2 j@3"],
    },
    // A3 with two red strands: isomorphic loadings, so non-minimal differentials
    CxFamily {
        ctx: "klrw type=A3 ring=QQ red=\"i@0, k@2\" quotient=cyclotomic",
        objects: &["i@1", "i@1 k@3", "k@3 i@4"],
    },
    // A2 with weights theta and two red strands
    CxFamily {
        ctx: "klrw type=A2 theta_ij=1/2 ring=QQ red=\"i@0, j@5/2\" quotient=cyclotomic",
        objects: &["i@1 j@3", "j@3 i@4", "i@3 j@4"],
    },
];

/// A one-dimensional complex at positions `lo..=0` in a family.
#[derive(Clone, Debug)]
struct CxCase {
    family: usize,
    ctx: String,
    terms: Vec<Sums>,
    diffs: Vec<Mat>,
}

impl CxCase {
    fn lo(&self) -> i64 {
        -(self.terms.len() as i64 - 1)
    }
    fn json(&self) -> Value {
        let lo = self.lo();
        json!({
            "terms": self.terms.iter().enumerate().map(|(t, s)| json!({
                "at": lo + t as i64, "summands": sums_json(s),
            })).collect::<Vec<_>>(),
            "differentials": self.diffs.iter().enumerate().map(|(t, m)| json!({
                "from": lo + t as i64, "matrix": mat_json(m),
            })).collect::<Vec<_>>(),
        })
    }
    fn objects(&self) -> BTreeSet<String> {
        let mut s: BTreeSet<String> = CX_FAMILIES[self.family]
            .objects
            .iter()
            .map(|o| o.to_string())
            .collect();
        s.extend(self.terms.iter().flatten().map(|(o, _)| o.clone()));
        s
    }
    /// A random map `C(p) → C(p + step)` of degree zero, one matrix per position.
    fn random_maps(&self, o: &mut Oracle, r: &mut Rng, step: i64, density: usize) -> Vec<Mat> {
        (0..self.terms.len())
            .map(|t| {
                let u = t as i64 + step;
                let tgt = if (0..self.terms.len() as i64).contains(&u) {
                    self.terms[u as usize].clone()
                } else {
                    vec![]
                };
                tgt.iter()
                    .map(|y| {
                        self.terms[t]
                            .iter()
                            .map(|x| combination(r, &o.hom(&x.0, &y.0, x.1 - y.1), density))
                            .collect()
                    })
                    .collect()
            })
            .collect()
    }
}

fn cx_case(seed: u64) -> CxCase {
    let mut r = Rng(seed | 1);
    let family = r.below(CX_FAMILIES.len());
    let f = &CX_FAMILIES[family];
    let mut o = Oracle::new(f.ctx, order_of(f.ctx));
    // C(0), then C(−1) with shifts placed so that maps of degree zero exist
    let c0: Sums = (0..1 + r.below(2))
        .map(|_| (r.pick(f.objects).to_string(), *r.pick(&[0i64, 1, 2])))
        .collect();
    let place = |o: &mut Oracle, r: &mut Rng, tgt: &Sums| -> Sums {
        (0..1 + r.below(2))
            .map(|_| {
                let x = r.pick(f.objects).to_string();
                let y = r.pick(tgt).clone();
                let ds = o.hom_degrees(&x, &y.0, -2, 4);
                let d = if ds.is_empty() { 0 } else { *r.pick(&ds) };
                (x, y.1 + d)
            })
            .collect()
    };
    let c1 = place(&mut o, &mut r, &c0);
    let random = |o: &mut Oracle, r: &mut Rng, src: &Sums, tgt: &Sums| -> Mat {
        tgt.iter()
            .map(|y| {
                src.iter()
                    .map(|x| combination(r, &o.hom(&x.0, &y.0, x.1 - y.1), 3))
                    .collect()
            })
            .collect()
    };
    let d1 = random(&mut o, &mut r, &c1, &c0);
    let mut case = CxCase {
        family,
        ctx: f.ctx.to_string(),
        terms: vec![c1, c0],
        diffs: vec![d1],
    };
    // a third term: summands T⟨d⟩ whose maps into C(−1) are random cycles of d
    if r.chance(1, 2) {
        let c1 = case.terms[0].clone();
        let mut c2 = Sums::new();
        let mut cols: Vec<Vec<String>> = Vec::new();
        for _ in 0..4 {
            if c2.len() == 2 {
                break;
            }
            let t = r.pick(f.objects).to_string();
            let y = r.pick(&c1).clone();
            let ds = o.hom_degrees(&t, &y.0, -2, 4);
            if ds.is_empty() {
                continue;
            }
            let d = y.1 + *r.pick(&ds);
            let sp = o.space(&t, d, &c1);
            let imgs: Vec<Elt> = sp
                .iter()
                .map(|c| o.apply(Some(&case.diffs[0]), c))
                .collect();
            let rels = relations_among(&imgs);
            if rels.is_empty() {
                continue;
            }
            // a random combination of the cycles
            let mut coef = vec![Q::int(0); sp.len()];
            for z in &rels {
                let c = Q::int(*r.pick(&[1i128, -1, 2]));
                for k in 0..sp.len() {
                    coef[k] = coef[k].add(z[k].mul(c));
                }
            }
            let col: Vec<String> = (0..c1.len())
                .map(|j| text_sum((0..sp.len()).map(|k| scaled(coef[k], &sp[k][j]))))
                .collect();
            c2.push((t, d));
            cols.push(col);
        }
        if !c2.is_empty() {
            let d2: Mat = (0..c1.len())
                .map(|j| cols.iter().map(|c| c[j].clone()).collect())
                .collect();
            case.terms.insert(0, c2);
            case.diffs.insert(0, d2);
        }
    }
    // a contractible pair Y⟨s⟩ --1--> Y⟨s⟩ at the last differential
    if r.chance(1, 3) {
        let y = (r.pick(f.objects).to_string(), *r.pick(&[0i64, 1, 3]));
        let k = case.terms.len();
        let e = format!("e({})", y.0);
        case.terms[k - 2].push(y.clone());
        case.terms[k - 1].push(y);
        let last = case.diffs.last_mut().unwrap();
        for row in last.iter_mut() {
            row.push(String::new());
        }
        let mut row = vec![String::new(); case.terms[k - 2].len()];
        *row.last_mut().unwrap() = e;
        last.push(row);
        if k == 3 {
            // nothing of C(−2) maps to the new summand
            let n = case.terms[0].len();
            case.diffs[0].push(vec![String::new(); n]);
        }
    }
    case
}

/// `{(test, degree) ↦ dim}` of the homology pieces at one position, as reported.
fn read_pieces(p: &Value) -> BTreeMap<(String, i64), i64> {
    p["pieces"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| {
            (
                (
                    x["test"].as_str().unwrap().to_string(),
                    x["degree"].as_i64().unwrap(),
                ),
                x["dimension"].as_i64().unwrap(),
            )
        })
        .collect()
}

fn window_of(v: &Value) -> (i64, i64, bool) {
    let w = &v["window"];
    if w.is_null() {
        (-4, 4, false)
    } else {
        (
            w["from"].as_i64().unwrap(),
            w["to"].as_i64().unwrap(),
            w["truncated"] == true,
        )
    }
}

/// The oracle's homology dimensions of a complex at index `t`, for the tests and degrees.
fn homology_dims(
    o: &mut Oracle,
    terms: &[Sums],
    diffs: &[Mat],
    t: usize,
    tests: &BTreeSet<String>,
    degrees: std::ops::RangeInclusive<i64>,
) -> BTreeMap<(String, i64), i64> {
    let mut out = BTreeMap::new();
    for test in tests {
        for d in degrees.clone() {
            let prev = (t > 0).then(|| (&terms[t - 1], &diffs[t - 1]));
            let next = diffs.get(t);
            let n = o.homology_dim(test, d, prev, &terms[t], next);
            if n > 0 {
                out.insert((test.clone(), d), n as i64);
            }
        }
    }
    out
}

/// Whether `m ∘ n = 0`, entry by entry.
fn composes_to_zero(o: &mut Oracle, m: &Mat, n: &Mat) -> bool {
    mat_mul(m, n).iter().flatten().all(|e| o.nf(e).is_empty())
}

/// What the checks of one case saw, for the sweep's coverage assertions.
#[derive(Default, Debug)]
struct CxSeen {
    family: usize,
    homology_zero: bool,
    homology_nonzero: bool,
    minimal: bool,
    not_minimal: bool,
    eliminated: bool,
    kernel_truncated: bool,
    kernel_complete: bool,
    dropped: bool,
    chainmap_holds: bool,
    chainmap_fails: bool,
    homotopy_holds: bool,
    homotopy_fails: bool,
    complex_fails: bool,
    module_nonzero: bool,
    resolution_extended: bool,
    homotopy_found: bool,
    homotopy_not_found: bool,
    chainmap_solved: bool,
    chainmap_unsolved: bool,
}

/// Checks that `gens` (columns into `src`, from `T⟨d⟩`) lie in `sub` and generate it at every
/// test object and degree, and that none lies in the submodule generated by the others. `sub`
/// gives the subspace at `(T, d)` as a spanning set of coordinates.
#[allow(clippy::type_complexity)]
fn check_generators(
    o: &mut Oracle,
    src: &Sums,
    gens: &[((String, i64), Vec<String>)],
    tests: &BTreeSet<String>,
    degrees: std::ops::RangeInclusive<i64>,
    sub: &mut dyn FnMut(&mut Oracle, &str, i64) -> Vec<Elt>,
    ctxt: &str,
) {
    let produced = |o: &mut Oracle, t: &str, d: i64, skip: Option<usize>| -> Vec<Elt> {
        let mut out = Vec::new();
        for (k, ((g, s), col)) in gens.iter().enumerate() {
            if Some(k) == skip {
                continue;
            }
            for b in o.hom(t, g, d - s) {
                let c: Vec<String> = col
                    .iter()
                    .map(|f| {
                        if f.is_empty() {
                            String::new()
                        } else {
                            format!("({f})({b})")
                        }
                    })
                    .collect();
                out.push(o.apply(None, &c));
            }
        }
        out
    };
    let _ = src;
    for t in tests {
        for d in degrees.clone() {
            let s = sub(o, t, d);
            let p = produced(o, t, d, None);
            let rs = rank_of(&s);
            let both: Vec<Elt> = s.iter().chain(&p).cloned().collect();
            assert_eq!(
                rank_of(&both),
                rs,
                "a generator outside at ({t}, {d}): {ctxt}"
            );
            assert_eq!(rank_of(&p), rs, "not generated at ({t}, {d}): {ctxt}");
        }
    }
    for (k, ((g, s), col)) in gens.iter().enumerate() {
        let own = o.apply(None, col);
        let others = produced(o, g, *s, Some(k));
        let mut with: Vec<Elt> = others.clone();
        with.push(own);
        assert!(
            rank_of(&with) > rank_of(&others),
            "generator {k} is produced by the others: {ctxt}"
        );
    }
}

/// A map of complexes as coordinates: (position index, row, column, power of `h`, monomial).
type MapElt = BTreeMap<(usize, usize, usize, u32, String), Q>;

fn map_add(a: &mut MapElt, t: usize, m: &Mat, c: Q, o: &mut Oracle) {
    for (i, row) in m.iter().enumerate() {
        for (j, e) in row.iter().enumerate() {
            for ((h, mon), q) in o.nf(e) {
                let k = a.entry((t, i, j, h, mon)).or_insert(Q::int(0));
                *k = k.add(q.mul(c));
            }
        }
    }
    a.retain(|_, q| !q.zero());
}

fn map_rank(vs: &[MapElt]) -> usize {
    let keys: Vec<_> = vs
        .iter()
        .flat_map(|v| v.keys().cloned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let rows: Vec<Vec<Q>> = vs
        .iter()
        .map(|v| {
            keys.iter()
                .map(|k| *v.get(k).unwrap_or(&Q::int(0)))
                .collect()
        })
        .collect();
    rank(&rows)
}

/// The basis of the maps `C(t) → C(t + step)` of internal degree `e` with one nonzero entry:
/// (t, the map as a matrix).
fn single_entry_maps(o: &mut Oracle, terms: &[Sums], step: i64, e: i64) -> Vec<(usize, Mat)> {
    let mut out = Vec::new();
    for t in 0..terms.len() {
        let u = t as i64 + step;
        if !(0..terms.len() as i64).contains(&u) {
            continue;
        }
        let (src, tgt) = (&terms[t], &terms[u as usize]);
        for (i, y) in tgt.iter().enumerate() {
            for (j, x) in src.iter().enumerate() {
                for b in o.hom(&x.0, &y.0, x.1 - y.1 + e) {
                    let mut m: Mat = vec![vec![String::new(); src.len()]; tgt.len()];
                    m[i][j] = b;
                    out.push((t, m));
                }
            }
        }
    }
    out
}

fn neg_texts(m: &Mat) -> Mat {
    m.iter()
        .map(|r| {
            r.iter()
                .map(|e| {
                    if e.is_empty() {
                        String::new()
                    } else {
                        format!("-({e})")
                    }
                })
                .collect()
        })
        .collect()
}

/// `d h + h d` for a single-entry `h : C(t) → C(t − 1)`, as coordinates.
fn dh_plus_hd(o: &mut Oracle, diffs: &[Mat], t: usize, h: &Mat) -> MapElt {
    let mut out = MapElt::new();
    // (d h)(t) = d(t − 1) h(t), and (h d)(t − 1) = h(t) d(t − 1)
    map_add(&mut out, t, &mat_mul(&diffs[t - 1], h), Q::int(1), o);
    map_add(&mut out, t - 1, &mat_mul(h, &diffs[t - 1]), Q::int(1), o);
    out
}

/// `d F − F d` square by square for a single-entry `F : C(t) → C(t)`, as coordinates indexed by
/// the source of the square.
fn chain_defect(o: &mut Oracle, diffs: &[Mat], t: usize, f: &Mat) -> MapElt {
    let mut out = MapElt::new();
    if t < diffs.len() {
        map_add(&mut out, t, &mat_mul(&diffs[t], f), Q::int(1), o);
    }
    if t > 0 {
        map_add(&mut out, t - 1, &mat_mul(f, &diffs[t - 1]), Q::int(-1), o);
    }
    out
}

/// Checks `homotopy.find`, `chainmap.solve`, and `complex.hom` on a case against the linear
/// algebra of single-entry maps, recording the branches reached in `seen`.
fn check_solvers(o: &mut Oracle, case: &CxCase, maps: &[(Vec<Mat>, bool)], seen: &mut CxSeen) {
    let ctx = case.ctx.as_str();
    let cx = case.json();
    let lo = case.lo();
    let k = case.terms.len();
    let ctxt = format!("{case:?}");
    let map_json = |m: &[Mat]| -> Value {
        json!(
            m.iter()
                .enumerate()
                .map(|(t, m)| json!({ "at": lo + t as i64, "matrix": mat_json(m) }))
                .collect::<Vec<_>>()
        )
    };
    // homotopy.find: h with d h + h d = f
    let hs = single_entry_maps(o, &case.terms, -1, 0);
    let cols: Vec<MapElt> = hs
        .iter()
        .map(|(t, h)| dh_plus_hd(o, &case.diffs, *t, h))
        .collect();
    let r = map_rank(&cols);
    for (f, _) in maps {
        let mut target = MapElt::new();
        for (t, m) in f.iter().enumerate() {
            map_add(&mut target, t, m, Q::int(1), o);
        }
        let mut with = cols.clone();
        with.push(target.clone());
        let exists = map_rank(&with) == r;
        let v = call(
            "homotopy.find",
            json!({ "context": ctx, "source": cx, "target": cx, "f": map_json(f) }),
        );
        assert_eq!(v["exists"], exists, "homotopy.find: {ctxt}\n{v:#}");
        if exists {
            seen.homotopy_found = true;
            assert_eq!(v["freedom"], hs.len() - r, "{ctxt}\n{v:#}");
            // the h returned satisfies d h + h d = f
            let mut got = MapElt::new();
            for c in v["h"].as_array().unwrap() {
                let t = (c["at"].as_i64().unwrap() - lo) as usize;
                let h = mat_read(&c["matrix"]);
                if t == 0 || h.is_empty() {
                    continue;
                }
                let e = dh_plus_hd(o, &case.diffs, t, &h);
                for (key, q) in e {
                    let x = got.entry(key).or_insert(Q::int(0));
                    *x = x.add(q);
                }
            }
            got.retain(|_, q| !q.zero());
            assert_eq!(got, target, "d h + h d ≠ f: {ctxt}\n{v:#}");
        } else {
            seen.homotopy_not_found = true;
        }
    }
    // chainmap.solve: F with F(0) given, the rest solved for
    let fs: Vec<(usize, Mat)> = single_entry_maps(o, &case.terms, 0, 0)
        .into_iter()
        .filter(|(t, _)| *t != k - 1)
        .collect();
    let cols: Vec<MapElt> = fs
        .iter()
        .map(|(t, f)| chain_defect(o, &case.diffs, *t, f))
        .collect();
    let r = map_rank(&cols);
    for (f, _) in maps {
        let fixed = &f[k - 1];
        // the fixed component's part of d F − F d, which the unknowns must cancel
        let mut target = MapElt::new();
        map_add(
            &mut target,
            k - 2,
            &mat_mul(fixed, &case.diffs[k - 2]),
            Q::int(1),
            o,
        );
        let mut with = cols.clone();
        with.push(target);
        let exists = map_rank(&with) == r;
        let v = call(
            "chainmap.solve",
            json!({ "context": ctx, "source": cx, "target": cx,
                "fixed": [{ "at": 0, "matrix": mat_json(fixed) }] }),
        );
        assert_eq!(v["exists"], exists, "chainmap.solve: {ctxt}\n{v:#}");
        assert_eq!(v["freedom"], fs.len() - r, "{ctxt}\n{v:#}");
        if exists {
            seen.chainmap_solved = true;
            let mut got: Vec<Mat> = (0..k)
                .map(|t| vec![vec![String::new(); case.terms[t].len()]; case.terms[t].len()])
                .collect();
            for c in v["map"].as_array().unwrap() {
                got[(c["at"].as_i64().unwrap() - lo) as usize] = mat_read(&c["matrix"]);
            }
            let mut fixed_back = MapElt::new();
            map_add(&mut fixed_back, 0, &got[k - 1], Q::int(1), o);
            let mut fixed_want = MapElt::new();
            map_add(&mut fixed_want, 0, fixed, Q::int(1), o);
            assert_eq!(fixed_back, fixed_want, "{ctxt}\n{v:#}");
            for t in 0..k - 1 {
                let a = mat_mul(&case.diffs[t], &got[t]);
                let b = mat_mul(&got[t + 1], &case.diffs[t]);
                let mut e = MapElt::new();
                map_add(&mut e, 0, &a, Q::int(1), o);
                map_add(&mut e, 0, &b, Q::int(-1), o);
                assert!(e.is_empty(), "not a chain map: {ctxt}\n{v:#}");
            }
        } else {
            seen.chainmap_unsolved = true;
        }
    }
    // complex.hom: H^n of Hom(C, C) in each internal degree of the window
    let v = call(
        "complex.hom",
        json!({ "context": ctx, "source": cx, "target": cx }),
    );
    let (from, to, truncated) = window_of(&v);
    let mut got = BTreeMap::new();
    for d in v["degrees"].as_array().unwrap() {
        for c in d["cohomology"].as_array().unwrap() {
            if c["dimension"].as_i64().unwrap() > 0 {
                got.insert(
                    (d["degree"].as_i64().unwrap(), c["n"].as_i64().unwrap()),
                    (
                        c["cochains"].as_i64().unwrap(),
                        c["dimension"].as_i64().unwrap(),
                    ),
                );
            }
        }
    }
    let mut expected = BTreeMap::new();
    let kk = k as i64;
    for e in from - 2..=if truncated { to } else { to + 2 } {
        // δ^n f = d f − (−1)^n f d, from Hom^n to Hom^(n+1), as coordinates by position
        let delta_rank = |o: &mut Oracle, n: i64| -> (usize, usize) {
            let us = single_entry_maps(o, &case.terms, n, e);
            let imgs: Vec<MapElt> = us
                .iter()
                .map(|(t, f)| {
                    let mut out = MapElt::new();
                    let u = (*t as i64 + n) as usize;
                    if u < k - 1 {
                        map_add(&mut out, *t, &mat_mul(&case.diffs[u], f), Q::int(1), o);
                    }
                    if *t > 0 {
                        let sign = if n.rem_euclid(2) == 0 { -1 } else { 1 };
                        map_add(
                            &mut out,
                            *t - 1,
                            &mat_mul(f, &case.diffs[*t - 1]),
                            Q::int(sign),
                            o,
                        );
                    }
                    out
                })
                .collect();
            (us.len(), map_rank(&imgs))
        };
        for n in -(kk - 1)..=kk - 1 {
            let (dim, r_out) = delta_rank(o, n);
            let (_, r_in) = delta_rank(o, n - 1);
            let h = dim - r_out - r_in;
            if h > 0 {
                expected.insert((e, n), (dim as i64, h as i64));
            }
        }
    }
    let in_window: BTreeMap<_, _> = expected
        .iter()
        .filter(|((e, _), _)| (from..=to).contains(e))
        .map(|(a, b)| (*a, *b))
        .collect();
    assert_eq!(
        in_window, expected,
        "Hom cohomology outside the window: {ctxt}\n{v:#}"
    );
    assert_eq!(got, expected, "complex.hom: {ctxt}\n{v:#}");
}

/// Runs the complex operations on a case and checks them against the oracle.
fn check_cx(case: &CxCase, seed: u64, o: &mut Oracle) -> CxSeen {
    let mut seen = CxSeen {
        family: case.family,
        ..CxSeen::default()
    };
    let ctx = case.ctx.as_str();
    assert_eq!(o.ctx, ctx);
    let o = &mut *o;
    let cx = case.json();
    let ctxt = format!("{case:?}");
    let lo = case.lo();
    // every object, for checks that hold at any test object
    let mut tests = case.objects();
    // one test object per order type, as the operations list them, for comparing dimensions
    let mut listed = BTreeSet::new();

    // complex.check: the case squares to zero by construction
    let v = call("complex.check", json!({ "context": ctx, "complex": cx }));
    assert_eq!(v["holds"], true, "{ctxt}\n{v:#}");
    for t in 0..case.diffs.len().saturating_sub(1) {
        assert!(
            composes_to_zero(o, &case.diffs[t + 1], &case.diffs[t]),
            "{ctxt}"
        );
    }

    // complex.homology at every position
    let mut hom_dims = Vec::new();
    for t in 0..case.terms.len() {
        let at = lo + t as i64;
        let v = call(
            "complex.homology",
            json!({ "context": ctx, "complex": cx, "at": at, "representatives": 2 }),
        );
        let p = &v["positions"][0];
        let (from, to, truncated) = window_of(p);
        let got = read_pieces(p);
        tests.extend(got.keys().map(|(t, _)| t.clone()));
        // the test objects the operation used, from its note
        let notes = v["provenance"]["notes"].as_array().unwrap();
        let listed_here = notes[0].as_str().unwrap();
        let listed_here = listed_here
            .strip_prefix("homology of Hom(T⟨d⟩, C) at the test objects T: ")
            .unwrap_or_else(|| panic!("{v:#}"));
        listed.extend(listed_here.split(", ").map(String::from));
        tests.extend(listed.iter().cloned());
        let expected = homology_dims(
            o,
            &case.terms,
            &case.diffs,
            t,
            &listed,
            from - 2..=if truncated { to } else { to + 2 },
        );
        assert_eq!(got, expected, "homology at {at}: {ctxt}\n{v:#}");
        // `zero`: nothing in the window; `exact`: nothing anywhere
        assert_eq!(p["zero"], got.is_empty(), "{ctxt}\n{v:#}");
        assert_eq!(v["exact"], got.is_empty() && !truncated, "{ctxt}\n{v:#}");
        if got.is_empty() {
            seen.homology_zero = true;
        } else {
            seen.homology_nonzero = true;
        }
        // representatives are cycles, independent modulo boundaries
        for piece in p["pieces"].as_array().unwrap() {
            let test = piece["test"].as_str().unwrap();
            let d = piece["degree"].as_i64().unwrap();
            let reps: Vec<Vec<String>> = piece["representatives"]
                .as_array()
                .unwrap()
                .iter()
                .map(|c| {
                    c.as_array()
                        .unwrap()
                        .iter()
                        .map(|f| f.as_str().unwrap_or("").to_string())
                        .collect()
                })
                .collect();
            let n = piece["dimension"].as_i64().unwrap() as usize;
            assert_eq!(reps.len(), n.min(2), "{ctxt}\n{v:#}");
            let mut bounds = Vec::new();
            if t > 0 {
                for c in o.space(test, d, &case.terms[t - 1]) {
                    bounds.push(o.apply(Some(&case.diffs[t - 1]), &c));
                }
            }
            let rb = rank_of(&bounds);
            let mut all = bounds.clone();
            for c in &reps {
                if let Some(g) = case.diffs.get(t) {
                    assert!(o.apply(Some(g), c).is_empty(), "not a cycle: {ctxt}\n{v:#}");
                }
                all.push(o.apply(None, c));
            }
            assert_eq!(rank_of(&all), rb + reps.len(), "{ctxt}\n{v:#}");
        }
        hom_dims.push((got, from - 2..=if truncated { to } else { to + 2 }));
    }

    // complex.euler: Σ (−1)^p q^s [X]
    let v = call("complex.euler", json!({ "context": ctx, "complex": cx }));
    let mut expected: BTreeMap<String, BTreeMap<i64, i64>> = BTreeMap::new();
    for (t, ss) in case.terms.iter().enumerate() {
        let sign = if (lo + t as i64).rem_euclid(2) == 0 {
            1
        } else {
            -1
        };
        for (x, s) in ss {
            *expected
                .entry(x.clone())
                .or_default()
                .entry(*s)
                .or_default() += sign;
        }
    }
    for m in expected.values_mut() {
        m.retain(|_, c| *c != 0);
    }
    expected.retain(|_, m| !m.is_empty());
    let got: BTreeMap<String, BTreeMap<i64, i64>> = v["terms"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| {
            (
                t["object"].as_str().unwrap().to_string(),
                t["powers"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|p| (p[0].as_i64().unwrap(), p[1].as_i64().unwrap()))
                    .collect(),
            )
        })
        .collect();
    assert_eq!(got, expected, "euler: {ctxt}\n{v:#}");

    // complex.minimal: every entry in the radical
    let mut ends = BTreeMap::new();
    let mut failures = 0;
    for (t, m) in case.diffs.iter().enumerate() {
        for (i, row) in m.iter().enumerate() {
            for (j, f) in row.iter().enumerate() {
                let (x, y) = (&case.terms[t][j], &case.terms[t + 1][i]);
                if !o.in_radical(&mut ends, x, y, f) {
                    failures += 1;
                }
            }
        }
    }
    let v = call("complex.minimal", json!({ "context": ctx, "complex": cx }));
    assert_eq!(v["holds"], failures == 0, "minimal: {ctxt}\n{v:#}");
    if failures == 0 {
        seen.minimal = true;
    } else {
        seen.not_minimal = true;
        assert!(v["failure_count"].as_i64().unwrap() > 0, "{ctxt}\n{v:#}");
    }

    // complex.gauss: a complex with the same homology, minimal when finished
    let v = call("complex.gauss", json!({ "context": ctx, "complex": cx }));
    let (glo, gterms, gdiffs) = cx_read(&v["complex"]);
    let n_in: usize = case.terms.iter().map(Vec::len).sum();
    let n_out: usize = gterms.iter().map(Vec::len).sum();
    let elim = v["eliminations"].as_array().unwrap().len();
    assert_eq!(n_in - n_out, 2 * elim, "{ctxt}\n{v:#}");
    if elim > 0 {
        seen.eliminated = true;
    }
    for t in 0..gdiffs.len().saturating_sub(1) {
        assert!(
            composes_to_zero(o, &gdiffs[t + 1], &gdiffs[t]),
            "{ctxt}\n{v:#}"
        );
    }
    for (t, (got, degrees)) in hom_dims.iter().enumerate() {
        let at = lo + t as i64;
        let g = at - glo;
        let mine = if (0..gterms.len() as i64).contains(&g) {
            homology_dims(o, &gterms, &gdiffs, g as usize, &listed, degrees.clone())
        } else {
            BTreeMap::new()
        };
        let mine_in = homology_dims(o, &case.terms, &case.diffs, t, &listed, degrees.clone());
        assert_eq!(&mine_in, got, "{ctxt}");
        assert_eq!(
            mine, mine_in,
            "gauss changed the homology at {at}: {ctxt}\n{v:#}"
        );
    }
    let local = gterms.iter().flatten().all(|(x, _)| {
        if !ends.contains_key(x) {
            let e = EndZero::new(o, x);
            ends.insert(x.clone(), e);
        }
        ends[x].simple_dim == 1
    });
    if v["finished"] == true && local {
        // with End⁰ of every summand local, an entry outside the radical is invertible, and
        // gauss removes it
        let mut ends2 = BTreeMap::new();
        for (t, m) in gdiffs.iter().enumerate() {
            for (i, row) in m.iter().enumerate() {
                for (j, f) in row.iter().enumerate() {
                    let (x, y) = (&gterms[t][j], &gterms[t + 1][i]);
                    assert!(
                        o.in_radical(&mut ends2, x, y, f),
                        "gauss finished with a non-radical entry: {ctxt}\n{v:#}"
                    );
                }
            }
        }
    }

    // complex.kernel at the far end: generators of ker d(lo)
    let v = call(
        "complex.kernel",
        json!({ "context": ctx, "complex": cx, "at": lo }),
    );
    let (from, to, truncated) = window_of(&v);
    assert_eq!(v["complete"], !truncated, "{ctxt}\n{v:#}");
    if truncated {
        seen.kernel_truncated = true;
    } else {
        seen.kernel_complete = true;
    }
    let notes = v["provenance"]["notes"].to_string();
    if notes.contains("found earlier in the degree sweep") {
        seen.dropped = true;
    }
    let ks = sums_read(&v["summands"]);
    let km = mat_read(&v["matrix"]);
    let gens: Vec<((String, i64), Vec<String>)> = ks
        .iter()
        .enumerate()
        .map(|(k, s)| (s.clone(), km.iter().map(|row| row[k].clone()).collect()))
        .collect();
    tests.extend(ks.iter().map(|s| s.0.clone()));
    let src = case.terms[0].clone();
    let d0 = case.diffs[0].clone();
    check_generators(
        o,
        &src,
        &gens,
        &tests,
        from - 2..=if truncated { to } else { to + 2 },
        &mut |o, t, d| {
            let sp = o.space(t, d, &src);
            let imgs: Vec<Elt> = sp.iter().map(|c| o.apply(Some(&d0), c)).collect();
            let basis: Vec<Elt> = sp.iter().map(|c| o.apply(None, c)).collect();
            relations_among(&imgs)
                .iter()
                .map(|z| {
                    let mut e = Elt::new();
                    for (k, b) in basis.iter().enumerate() {
                        elt_add(&mut e, b, z[k]);
                    }
                    e
                })
                .collect()
        },
        &format!("kernel: {ctxt}\n{v:#}"),
    );

    // chain maps and homotopies C → C: f = d h + h d (+ 1) is a chain map, homotopic to 0 (or 1)
    let mut r = Rng(seed.rotate_left(17) | 1);
    let h = case.random_maps(o, &mut r, -1, 2);
    let k = case.terms.len();
    let mut f: Vec<Mat> = (0..k)
        .map(|t| {
            let mut m: Mat = vec![vec![String::new(); case.terms[t].len()]; case.terms[t].len()];
            if t + 1 < k {
                // h(p+1) d(p)
                m = mat_add(&m, &mat_mul(&h[t + 1], &case.diffs[t]));
            }
            if t > 0 {
                // d(p−1) h(p)
                m = mat_add(&m, &mat_mul(&case.diffs[t - 1], &h[t]));
            }
            m
        })
        .collect();
    let plus_one = r.chance(1, 2);
    if plus_one {
        for (t, m) in f.iter_mut().enumerate() {
            for (i, row) in m.iter_mut().enumerate() {
                let e = format!("e({})", case.terms[t][i].0);
                row[i] = text_sum([row[i].clone(), e]);
            }
        }
    }
    // perturb one entry sometimes
    let mut perturbed = f.clone();
    let p = r.below(k);
    let i = r.below(case.terms[p].len());
    let j = r.below(case.terms[p].len());
    let x = &case.terms[p][j];
    let y = &case.terms[p][i];
    let extra = combination(&mut r, &o.hom(&x.0, &y.0, x.1 - y.1), 4);
    perturbed[p][i][j] = text_sum([perturbed[p][i][j].clone(), extra]);
    let map_json = |m: &[Mat]| -> Value {
        json!(
            m.iter()
                .enumerate()
                .map(|(t, m)| json!({ "at": lo + t as i64, "matrix": mat_json(m) }))
                .collect::<Vec<_>>()
        )
    };
    let commutes = |o: &mut Oracle, m: &[Mat]| -> bool {
        (0..k - 1).all(|t| {
            let a = mat_mul(&case.diffs[t], &m[t]);
            let b = mat_mul(&m[t + 1], &case.diffs[t]);
            a.iter().flatten().zip(b.iter().flatten()).all(|(x, y)| {
                let (lx, ly) = (o.nf(x), o.nf(y));
                lx == ly
            })
        })
    };
    for m in [&f, &perturbed] {
        let v = call(
            "chainmap.check",
            json!({ "context": ctx, "source": cx, "target": cx, "map": map_json(m) }),
        );
        let holds = commutes(o, m);
        assert_eq!(v["holds"], holds, "chainmap.check: {ctxt}\n{v:#}");
        if holds {
            seen.chainmap_holds = true;
        } else {
            seen.chainmap_fails = true;
        }
        // homotopy.check f − g = d h + h d, with g = 1 or 0
        let mut req = json!({ "context": ctx, "source": cx, "target": cx,
            "f": map_json(m), "h": map_json(&h) });
        if plus_one {
            let one: Vec<Mat> = case
                .terms
                .iter()
                .map(|ss| {
                    (0..ss.len())
                        .map(|i| {
                            (0..ss.len())
                                .map(|j| {
                                    if i == j {
                                        format!("e({})", ss[i].0)
                                    } else {
                                        String::new()
                                    }
                                })
                                .collect()
                        })
                        .collect()
                })
                .collect();
            req["g"] = map_json(&one);
        }
        let v = call("homotopy.check", req);
        let same = std::ptr::eq(m, &f)
            || (0..k).all(|t| {
                m[t].iter()
                    .flatten()
                    .zip(f[t].iter().flatten())
                    .all(|(a, b)| o.nf(a) == o.nf(b))
            });
        assert_eq!(v["holds"], same, "homotopy.check: {ctxt}\n{v:#}");
        if same {
            seen.homotopy_holds = true;
        } else {
            seen.homotopy_fails = true;
        }
    }

    // homotopy.find, chainmap.solve, and complex.hom; with 1 added, f is homotopic to 1, not 0
    let maps = vec![(f.clone(), true), (perturbed.clone(), false)];
    check_solvers(o, case, &maps, &mut seen);

    // complex.cone of f: [[−d, 0], [f, d]]; a cone of a map homotopic to 1 is acyclic
    let v = call(
        "complex.cone",
        json!({ "context": ctx, "source": cx, "target": cx, "map": map_json(&f) }),
    );
    let (clo, cterms, cdiffs) = cx_read(&v["complex"]);
    assert_eq!(clo, lo - 1, "{ctxt}\n{v:#}");
    for (t, ss) in cterms.iter().enumerate() {
        let p = clo + t as i64;
        let mut want = Sums::new();
        if p < 0 && p + 1 >= lo {
            want.extend(case.terms[(p + 1 - lo) as usize].clone());
        }
        if p >= lo {
            want.extend(case.terms[(p - lo) as usize].clone());
        }
        assert_eq!(ss, &want, "cone term at {p}: {ctxt}\n{v:#}");
    }
    for (t, m) in cdiffs.iter().enumerate() {
        let p = clo + t as i64;
        // blocks: source C(p+1) ⊕ C(p), target C(p+2) ⊕ C(p+1)
        let a = if p < 0 {
            case.terms[(p + 1 - lo) as usize].len()
        } else {
            0
        };
        let a2 = if p + 2 <= 0 {
            case.terms[(p + 2 - lo) as usize].len()
        } else {
            0
        };
        for (i, row) in m.iter().enumerate() {
            for (j, e) in row.iter().enumerate() {
                let want = match (i < a2, j < a) {
                    (true, true) => format!("-({})", case.diffs[(p + 1 - lo) as usize][i][j]),
                    (true, false) => String::new(),
                    (false, true) => f[(p + 1 - lo) as usize][i - a2][j].clone(),
                    (false, false) => case.diffs[(p - lo) as usize][i - a2][j - a].clone(),
                };
                let want = if want == "-()" { String::new() } else { want };
                assert_eq!(
                    o.nf(e),
                    o.nf(&want),
                    "cone entry ({i}, {j}) at {p}: {ctxt}\n{v:#}"
                );
            }
        }
    }
    if plus_one {
        for t in 0..cterms.len() {
            let dims = homology_dims(o, &cterms, &cdiffs, t, &tests, -6..=10);
            assert!(
                dims.is_empty(),
                "the cone of a map homotopic to 1 has homology: {ctxt}"
            );
        }
    }

    // complex.totalize of the square X(p, −1) = C(p) --f--> X(p, 0) = C(p): Tot(n) is the sum
    // over p + q = n, with d along axis 0 and (−1)^p f along axis 1
    let mut xterms = Vec::new();
    let mut xdiffs = Vec::new();
    for (t, ss) in case.terms.iter().enumerate() {
        let p = lo + t as i64;
        for q in [-1i64, 0] {
            xterms.push(json!({ "at": [p, q], "summands": sums_json(ss) }));
            if t + 1 < k {
                xdiffs
                    .push(json!({ "from": [p, q], "axis": 0, "matrix": mat_json(&case.diffs[t]) }));
            }
        }
        xdiffs.push(json!({ "from": [p, -1], "axis": 1, "matrix": mat_json(&f[t]) }));
    }
    let x = json!({ "dimension": 2, "terms": xterms, "differentials": xdiffs });
    let v = call("complex.totalize", json!({ "context": ctx, "complex": x }));
    let (tlo, tterms, tdiffs) = cx_read(&v["complex"]);
    // where each position's summands start in each total term
    let mut start: BTreeMap<(i64, i64), (usize, usize)> = BTreeMap::new();
    for b in v["blocks"].as_array().unwrap() {
        let n = b["at"].as_i64().unwrap();
        for e in b["blocks"].as_array().unwrap() {
            let pq = (
                e["from"][0].as_i64().unwrap(),
                e["from"][1].as_i64().unwrap(),
            );
            assert_eq!(pq.0 + pq.1, n, "{ctxt}\n{v:#}");
            start.insert(
                pq,
                ((n - tlo) as usize, e["first"].as_u64().unwrap() as usize),
            );
        }
    }
    for t in 0..k {
        for q in [-1i64, 0] {
            let (n, first) = start[&(lo + t as i64, q)];
            let got = &tterms[n][first..first + case.terms[t].len()];
            assert_eq!(got, &case.terms[t][..], "totalize term: {ctxt}\n{v:#}");
        }
    }
    let total: usize = tterms.iter().map(Vec::len).sum();
    assert_eq!(
        total,
        2 * case.terms.iter().map(Vec::len).sum::<usize>(),
        "{ctxt}\n{v:#}"
    );
    // every block of the total differential
    for ((p, q), (n, first)) in &start {
        let t = (p - lo) as usize;
        if *n >= tdiffs.len() {
            continue;
        }
        for ((p2, q2), (n2, first2)) in &start {
            if *n2 != n + 1 {
                continue;
            }
            let t2 = (p2 - lo) as usize;
            let want: Mat = if (*p2, *q2) == (p + 1, *q) {
                case.diffs[t].clone()
            } else if (*p2, *q2) == (*p, q + 1) {
                if p.rem_euclid(2) == 0 {
                    f[t].clone()
                } else {
                    neg_texts(&f[t])
                }
            } else {
                vec![vec![String::new(); case.terms[t].len()]; case.terms[t2].len()]
            };
            for (i, row) in want.iter().enumerate() {
                for (j, w) in row.iter().enumerate() {
                    let e = &tdiffs[*n][first2 + i][first + j];
                    assert_eq!(
                        o.nf(e),
                        o.nf(w),
                        "totalize block {p},{q} → {p2},{q2}: {ctxt}\n{v:#}"
                    );
                }
            }
        }
    }
    for t in 0..tdiffs.len().saturating_sub(1) {
        assert!(
            composes_to_zero(o, &tdiffs[t + 1], &tdiffs[t]),
            "{ctxt}\n{v:#}"
        );
    }
    if plus_one {
        for t in 0..tterms.len() {
            let dims = homology_dims(o, &tterms, &tdiffs, t, &tests, -6..=10);
            assert!(
                dims.is_empty(),
                "Tot of a map homotopic to 1 has homology: {ctxt}"
            );
        }
    }

    // complex.shift by k positions and s degrees: C[k]⟨s⟩(p) = C(p + k)⟨s⟩, d · (−1)^k
    let (by, grading) = (*r.pick(&[-1i64, 0, 1, 2]), *r.pick(&[0i64, 1, -2]));
    let v = call(
        "complex.shift",
        json!({ "context": ctx, "complex": cx, "by": by, "grading": grading }),
    );
    let (slo, sterms, sdiffs) = cx_read(&v["complex"]);
    assert_eq!(slo, lo - by, "{ctxt}\n{v:#}");
    for (t, ss) in sterms.iter().enumerate() {
        let want: Sums = case.terms[t]
            .iter()
            .map(|(x, s)| (x.clone(), s + grading))
            .collect();
        assert_eq!(ss, &want, "{ctxt}\n{v:#}");
    }
    for (t, m) in sdiffs.iter().enumerate() {
        let want = if by.rem_euclid(2) == 0 {
            case.diffs[t].clone()
        } else {
            neg_texts(&case.diffs[t])
        };
        for (e, w) in m.iter().flatten().zip(want.iter().flatten()) {
            assert_eq!(o.nf(e), o.nf(w), "shift: {ctxt}\n{v:#}");
        }
    }

    // module.check and resolutions of M = coker(d : C(−1) → C(0))
    let (q1, q0) = (&case.terms[k - 2], &case.terms[k - 1]);
    let phi = &case.diffs[k - 2];
    let module =
        json!({ "generators": sums_json(q0), "relations": sums_json(q1), "matrix": mat_json(phi) });
    let v = call("module.check", json!({ "context": ctx, "module": module }));
    assert_eq!(v["holds"], true, "{ctxt}\n{v:#}");
    let (from, to, truncated) = window_of(&v);
    let got = read_dims(&v["dimensions"]);
    let mut expected = BTreeMap::new();
    for t in &listed {
        for d in from - 2..=if truncated { to } else { to + 2 } {
            let n = o.homology_dim(t, d, Some((q1, phi)), q0, None);
            if n > 0 {
                expected.insert((t.clone(), d), n as i64);
            }
        }
    }
    assert_eq!(got, expected, "module dimensions: {ctxt}\n{v:#}");
    if !got.is_empty() {
        seen.module_nonzero = true;
    }
    let v = call(
        "resolution.start",
        json!({ "context": ctx, "module": module }),
    );
    let mut res = v["resolution"].clone();
    for step in 0..2 {
        let v = call(
            "resolution.extend",
            json!({ "context": ctx, "resolution": res }),
        );
        let added = sums_read(&v["added"]["summands"]);
        if added.is_empty() {
            break;
        }
        seen.resolution_extended = true;
        if v["provenance"]["notes"]
            .to_string()
            .contains("found earlier in the degree sweep")
        {
            seen.dropped = true;
        }
        let (rlo, rterms, rdiffs) = cx_read(&v["resolution"]);
        let src = rterms[1].clone();
        let m = rdiffs[0].clone();
        let gens: Vec<((String, i64), Vec<String>)> = added
            .iter()
            .enumerate()
            .map(|(k, s)| (s.clone(), m.iter().map(|row| row[k].clone()).collect()))
            .collect();
        tests.extend(added.iter().map(|s| s.0.clone()));
        let (from, to, truncated) = window_of(&v["added"]);
        let degrees = from - 2..=if truncated { to } else { to + 2 };
        let ctxt2 = format!("resolution step {step}: {ctxt}\n{v:#}");
        if rlo == -1 {
            // the kernel of C₀ → M: maps into Q₀ landing in the image of φ (ε = 1)
            let (q1, phi) = (q1.clone(), phi.clone());
            check_generators(
                o,
                &src,
                &gens,
                &tests,
                degrees,
                &mut |o, t, d| {
                    o.space(t, d, &q1)
                        .iter()
                        .map(|c| o.apply(Some(&phi), c))
                        .collect()
                },
                &ctxt2,
            );
        } else {
            let next = rdiffs[1].clone();
            check_generators(
                o,
                &src,
                &gens,
                &tests,
                degrees,
                &mut |o, t, d| {
                    let sp = o.space(t, d, &src);
                    let imgs: Vec<Elt> = sp.iter().map(|c| o.apply(Some(&next), c)).collect();
                    let basis: Vec<Elt> = sp.iter().map(|c| o.apply(None, c)).collect();
                    relations_among(&imgs)
                        .iter()
                        .map(|z| {
                            let mut e = Elt::new();
                            for (k, b) in basis.iter().enumerate() {
                                elt_add(&mut e, b, z[k]);
                            }
                            e
                        })
                        .collect()
                },
                &ctxt2,
            );
        }
        assert_eq!(v["check"]["holds"], true, "{ctxt2}");
        res = v["resolution"].clone();
    }
    let v = call(
        "resolution.check",
        json!({ "context": ctx, "resolution": res }),
    );
    assert_eq!(v["holds"], true, "{ctxt}\n{v:#}");

    // a corrupted complex: d(lo + 1) ∘ d(lo) ≠ 0 is reported
    if k == 3 {
        let mut bad = case.clone();
        let x = &bad.terms[0][0];
        let y = &bad.terms[1][0];
        let extra = combination(&mut r, &o.hom(&x.0, &y.0, x.1 - y.1), 4);
        bad.diffs[0][0][0] = text_sum([bad.diffs[0][0][0].clone(), extra]);
        let ok = composes_to_zero(o, &bad.diffs[1], &bad.diffs[0]);
        let v = call(
            "complex.check",
            json!({ "context": ctx, "complex": bad.json() }),
        );
        assert_eq!(v["holds"], ok, "{ctxt}\n{v:#}");
        if !ok {
            seen.complex_fails = true;
        }
    }
    seen
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 12, failure_persistence: None, rng_seed: FIXED_SEED, ..ProptestConfig::default() })]

    #[test]
    fn complex_operations_agree_with_brute_force(seed in any::<u64>()) {
        let case = cx_case(seed);
        let mut o = Oracle::new(&case.ctx, order_of(&case.ctx));
        check_cx(&case, seed, &mut o);
    }
}

/// A fixed sweep, which also asserts that every branch of every checked operation is reached,
/// in every family.
#[test]
fn complex_sweep_covers_every_branch() {
    let mut all: Vec<CxSeen> = Vec::new();
    // one oracle per context, so straightened terms are reused across cases
    let mut oracles: BTreeMap<String, Oracle> = BTreeMap::new();
    for seed in 1..80u64 {
        let s = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15);
        let case = cx_case(s);
        let o = oracles
            .entry(case.ctx.clone())
            .or_insert_with(|| Oracle::new(&case.ctx, order_of(&case.ctx)));
        all.push(check_cx(&case, s, o));
    }
    for (fam, f) in CX_FAMILIES.iter().enumerate() {
        assert!(all.iter().any(|s| s.family == fam), "no case in {}", f.ctx);
    }
    let any = |p: fn(&CxSeen) -> bool, what: &str| {
        assert!(all.iter().any(p), "no case with {what}");
    };
    any(|s| s.homology_zero, "zero homology");
    any(|s| s.homology_nonzero, "nonzero homology");
    any(|s| s.minimal, "a minimal complex");
    any(|s| s.not_minimal, "a non-minimal complex");
    any(|s| s.eliminated, "a gauss elimination");
    any(|s| s.kernel_truncated, "a truncated kernel window");
    any(|s| s.kernel_complete, "a complete kernel");
    any(|s| s.dropped, "a dropped generator");
    any(|s| !s.dropped, "no dropped generator");
    any(|s| s.chainmap_holds, "a chain map");
    any(|s| s.chainmap_fails, "a map that is not a chain map");
    any(|s| s.homotopy_holds, "a homotopy that holds");
    any(|s| s.homotopy_fails, "a homotopy that fails");
    any(|s| s.complex_fails, "a corrupted complex");
    any(|s| s.module_nonzero, "a nonzero module");
    any(|s| s.resolution_extended, "a resolution step");
    any(|s| s.homotopy_found, "a homotopy found");
    any(|s| s.homotopy_not_found, "no homotopy");
    any(|s| s.chainmap_solved, "a chain map solved for");
    any(
        |s| s.chainmap_unsolved,
        "no chain map extending the fixed component",
    );
    // the families with two red strands reach non-minimal and nonzero cases too
    for fam in [4usize, 5, 6] {
        assert!(
            all.iter().any(|s| s.family == fam && s.homology_nonzero),
            "family {fam}"
        );
    }
}
