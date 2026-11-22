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

#![allow(clippy::needless_range_loop)] // the exact linear algebra below indexes by row and column

use diagcalc_core::ops;
use proptest::prelude::*;
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
        let mut keys: BTreeSet<(usize, usize, usize, String)> = BTreeSet::new();
        for col in &self.cols {
            keys.extend(col.keys().filter(|k| k.0 < upto).cloned());
        }
        for (t, sq) in self.dd.iter().enumerate().take(upto) {
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
            // the printed entry is the first nonzero entry of [d²]_m at that square
            let detail = f["detail"].as_str().unwrap();
            let entry = detail
                .split_once("has the entry ")
                .and_then(|(_, r)| r.split_once("; it is"))
                .map(|(e, _)| e)
                .unwrap_or_else(|| panic!("no entry in {detail:?}"));
            let first = sys.dd[t]
                .iter()
                .flatten()
                .find(|e| !e.is_empty())
                .expect("nonzero");
            assert_eq!(&part(&o.nf(entry), 0), first, "{ctxt}");
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
    #![proptest_config(ProptestConfig { cases: 48, failure_persistence: None, ..ProptestConfig::default() })]

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
    let mut creq = json!({ "context": case.ctx, "module": module });
    if let Some(m) = case.max_degree {
        creq["max_degree"] = json!(m);
    }
    let c = call("module.check", creq);
    assert_eq!(c["holds"], true, "{ctxt}\n{c:#}");
    let checked: BTreeMap<_, _> = read_dims(&c["dimensions"])
        .into_iter()
        .filter(|((_, d), _)| (from..=to).contains(d))
        .collect();
    assert_eq!(checked, expected, "module.check of the head: {ctxt}\n{c:#}");
    (fired, truncated)
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 32, failure_persistence: None, ..ProptestConfig::default() })]

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
