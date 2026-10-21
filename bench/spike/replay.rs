//! Arithmetic spike: replays a trace of polynomial operations (`diagcalc-bench --trace`) on the
//! engine's current polynomial type and on a packed-monomial prototype, timing each kind of
//! operation. `flint_replay.c` replays the same trace on FLINT. All replays print the same
//! checksum of their results, so they can be compared.
//!
//! ```text
//! spike-replay TRACE [current|packed|both]
//! ```

use diagcalc_core::num::Int;
use diagcalc_core::poly::{Monomial, Poly, PolyRing};
use diagcalc_core::ring::{Integers, Ring};
use std::hint::black_box;
use std::io::{BufRead, BufReader};
use std::time::Instant;

/// A parsed operand: terms as (coefficient, exponents).
type Raw = Vec<(i64, Vec<i32>)>;

enum Op {
    Mul(usize, Raw, Raw),
    Add(usize, Raw, Raw),
    Div(usize, usize, usize, Raw),
}

fn parse(line: &str) -> Op {
    let mut t = line.split_ascii_whitespace();
    let op = t.next().unwrap();
    let n: usize = t.next().unwrap().parse().unwrap();
    let poly = |t: &mut std::str::SplitAsciiWhitespace| -> Raw {
        let len: usize = t.next().unwrap().parse().unwrap();
        (0..len)
            .map(|_| {
                let c: i64 = t.next().unwrap().parse().unwrap();
                let e = (0..n).map(|_| t.next().unwrap().parse().unwrap()).collect();
                (c, e)
            })
            .collect()
    };
    match op {
        "mul" => Op::Mul(n, poly(&mut t), poly(&mut t)),
        "add" => Op::Add(n, poly(&mut t), poly(&mut t)),
        "div" => {
            let i = t.next().unwrap().parse().unwrap();
            let j = t.next().unwrap().parse().unwrap();
            Op::Div(n, i, j, poly(&mut t))
        }
        _ => panic!("unknown op {op}"),
    }
}

/// The checksum all replays agree on: per result term, coefficient times a hash of the
/// exponents, summed with wrapping; a failed division adds 7.
fn term_hash(c: i64, e: impl Iterator<Item = i64>) -> u64 {
    let h = e.fold(0u64, |h, x| h.wrapping_mul(131).wrapping_add(x as u64));
    (c as u64).wrapping_mul(h.wrapping_add(1))
}

/// A polynomial backend under test.
trait Backend {
    type P;
    fn name(&self) -> &'static str;
    fn convert(&mut self, n: usize, p: &Raw) -> Self::P;
    fn mul(&self, n: usize, a: &Self::P, b: &Self::P) -> Self::P;
    fn add(&self, n: usize, a: &Self::P, b: &Self::P) -> Self::P;
    fn div(&self, n: usize, i: usize, j: usize, a: &Self::P) -> Option<Self::P>;
    fn checksum(&self, n: usize, p: &Self::P) -> u64;
}

// ---- the engine's polynomials ----

struct Current {
    rings: Vec<PolyRing<Integers>>,
}

impl Current {
    fn new() -> Self {
        let rings = (0..9)
            .map(|n| {
                let names: Vec<String> = (0..n).map(|i| format!("x{i}")).collect();
                let names: Vec<&str> = names.iter().map(String::as_str).collect();
                PolyRing::new(Integers, &names)
            })
            .collect();
        Current { rings }
    }
}

impl Backend for Current {
    type P = Poly<Int>;
    fn name(&self) -> &'static str {
        "current"
    }
    fn convert(&mut self, n: usize, p: &Raw) -> Self::P {
        let terms = p
            .iter()
            .map(|(c, e)| (Monomial::from(e.as_slice()), Int::from(*c)))
            .collect();
        self.rings[n].normalize(terms)
    }
    fn mul(&self, n: usize, a: &Self::P, b: &Self::P) -> Self::P {
        self.rings[n].mul(a, b)
    }
    fn add(&self, n: usize, a: &Self::P, b: &Self::P) -> Self::P {
        self.rings[n].add(a, b)
    }
    fn div(&self, n: usize, i: usize, j: usize, a: &Self::P) -> Option<Self::P> {
        self.rings[n].div_by_difference(a, i, j)
    }
    fn checksum(&self, _n: usize, p: &Self::P) -> u64 {
        p.terms.iter().fold(0u64, |s, (m, c)| {
            let c = c.to_i64().expect("coefficient fits i64");
            s.wrapping_add(term_hash(c, m.0.iter().map(|&x| x as i64)))
        })
    }
}

// ---- packed prototype: one u64 per monomial, i64 coefficients ----
//
// Byte 7 holds the total degree and bytes 6, 5, … the exponents of x_0, x_1, …, so comparing
// the integers compares monomials in graded lexicographic order, and multiplying monomials adds
// the integers. Up to 7 variables with exponents below 256; coefficients use checked i64
// arithmetic (a real implementation would fall back to big integers).

struct Packed;

type PPoly = Vec<(u64, i64)>;

const DEG: u64 = 1 << 56;

fn unit(v: usize) -> u64 {
    1u64 << (48 - 8 * v)
}

fn exp(m: u64, v: usize) -> i64 {
    ((m >> (48 - 8 * v)) & 0xff) as i64
}

/// Sorts by decreasing monomial and merges equal monomials, dropping zeros.
fn canon(mut t: PPoly) -> PPoly {
    t.sort_unstable_by_key(|x| std::cmp::Reverse(x.0));
    let mut out: PPoly = Vec::with_capacity(t.len());
    for (m, c) in t {
        match out.last_mut() {
            Some((lm, lc)) if *lm == m => *lc = lc.checked_add(c).expect("overflow"),
            _ => out.push((m, c)),
        }
    }
    out.retain(|(_, c)| *c != 0);
    out
}

impl Backend for Packed {
    type P = PPoly;
    fn name(&self) -> &'static str {
        "packed"
    }
    fn convert(&mut self, n: usize, p: &Raw) -> Self::P {
        assert!(n <= 7);
        canon(
            p.iter()
                .map(|(c, e)| {
                    let mut m = 0u64;
                    for (v, &x) in e.iter().enumerate() {
                        assert!((0..256).contains(&x));
                        m += x as u64 * unit(v) + x as u64 * DEG;
                    }
                    (m, *c)
                })
                .collect(),
        )
    }
    fn mul(&self, _n: usize, a: &PPoly, b: &PPoly) -> PPoly {
        if a.len() == 1 || b.len() == 1 {
            // multiplying by a term preserves the order
            let ((m, c), other) = if a.len() == 1 { (a[0], b) } else { (b[0], a) };
            return other
                .iter()
                .map(|&(m2, c2)| (m + m2, c.checked_mul(c2).expect("overflow")))
                .collect();
        }
        let mut t = Vec::with_capacity(a.len() * b.len());
        for &(ma, ca) in a {
            for &(mb, cb) in b {
                t.push((ma + mb, ca.checked_mul(cb).expect("overflow")));
            }
        }
        canon(t)
    }
    fn add(&self, _n: usize, a: &PPoly, b: &PPoly) -> PPoly {
        let (mut i, mut j) = (0, 0);
        let mut out = Vec::with_capacity(a.len() + b.len());
        while i < a.len() && j < b.len() {
            match a[i].0.cmp(&b[j].0) {
                std::cmp::Ordering::Greater => {
                    out.push(a[i]);
                    i += 1;
                }
                std::cmp::Ordering::Less => {
                    out.push(b[j]);
                    j += 1;
                }
                std::cmp::Ordering::Equal => {
                    let c = a[i].1.checked_add(b[j].1).expect("overflow");
                    if c != 0 {
                        out.push((a[i].0, c));
                    }
                    i += 1;
                    j += 1;
                }
            }
        }
        out.extend_from_slice(&a[i..]);
        out.extend_from_slice(&b[j..]);
        out
    }
    fn div(&self, _n: usize, i: usize, j: usize, a: &PPoly) -> Option<PPoly> {
        // exact iff a(x_i := x_j) = 0; check that first, then divide
        let (ui, uj) = (unit(i), unit(j));
        let rem = canon(
            a.iter()
                .map(|&(m, c)| {
                    let k = exp(m, i) as u64;
                    (m - k * ui + k * uj, c)
                })
                .collect(),
        );
        if !rem.is_empty() {
            return None;
        }
        let mut q = Vec::new();
        for &(m, c) in a {
            let k = exp(m, i) as u64;
            for l in 0..k {
                q.push((m - (k - l) * ui + (k - 1 - l) * uj - DEG, c));
            }
        }
        Some(canon(q))
    }
    fn checksum(&self, n: usize, p: &PPoly) -> u64 {
        p.iter().fold(0u64, |s, &(m, c)| {
            s.wrapping_add(term_hash(c, (0..n).map(|v| exp(m, v))))
        })
    }
}

#[derive(Default)]
struct Totals {
    count: [usize; 3],
    secs: [f64; 3],
    checksum: u64,
    failed_divs: usize,
}

/// Replays one chunk: converts the operands, times each kind of operation, then checksums.
fn replay_chunk<B: Backend>(b: &mut B, ops: &[Op], tot: &mut Totals) {
    let mut muls = Vec::new();
    let mut adds = Vec::new();
    let mut divs = Vec::new();
    for op in ops {
        match op {
            Op::Mul(n, x, y) => muls.push((*n, b.convert(*n, x), b.convert(*n, y))),
            Op::Add(n, x, y) => adds.push((*n, b.convert(*n, x), b.convert(*n, y))),
            Op::Div(n, i, j, x) => divs.push((*n, *i, *j, b.convert(*n, x))),
        }
    }
    let t = Instant::now();
    for (n, x, y) in &muls {
        black_box(b.mul(*n, x, y));
    }
    tot.secs[0] += t.elapsed().as_secs_f64();
    let t = Instant::now();
    for (n, x, y) in &adds {
        black_box(b.add(*n, x, y));
    }
    tot.secs[1] += t.elapsed().as_secs_f64();
    let t = Instant::now();
    for (n, i, j, x) in &divs {
        black_box(b.div(*n, *i, *j, x));
    }
    tot.secs[2] += t.elapsed().as_secs_f64();
    tot.count[0] += muls.len();
    tot.count[1] += adds.len();
    tot.count[2] += divs.len();
    // checksum in trace order: muls, then adds, then divs within each chunk (as flint_replay.c)
    for (n, x, y) in &muls {
        tot.checksum = tot.checksum.wrapping_add(b.checksum(*n, &b.mul(*n, x, y)));
    }
    for (n, x, y) in &adds {
        tot.checksum = tot.checksum.wrapping_add(b.checksum(*n, &b.add(*n, x, y)));
    }
    for (n, i, j, x) in &divs {
        match b.div(*n, *i, *j, x) {
            Some(q) => tot.checksum = tot.checksum.wrapping_add(b.checksum(*n, &q)),
            None => {
                tot.checksum = tot.checksum.wrapping_add(7);
                tot.failed_divs += 1;
            }
        }
    }
}

const CHUNK: usize = 100_000;

fn run<B: Backend>(path: &str, mut b: B) {
    let file = std::fs::File::open(path).expect("open trace");
    let mut tot = Totals::default();
    let mut ops = Vec::with_capacity(CHUNK);
    for line in BufReader::new(file).lines() {
        ops.push(parse(&line.expect("read trace")));
        if ops.len() == CHUNK {
            replay_chunk(&mut b, &ops, &mut tot);
            ops.clear();
        }
    }
    replay_chunk(&mut b, &ops, &mut tot);
    let all: f64 = tot.secs.iter().sum();
    println!(
        "{{\"backend\": \"{}\", \"platform\": \"{}\", \"mul\": [{}, {:.4}], \"add\": [{}, {:.4}], \
         \"div\": [{}, {:.4}], \"failed_divs\": {}, \"total_seconds\": {all:.4}, \"checksum\": \"{:016x}\"}}",
        b.name(),
        if cfg!(target_family = "wasm") {
            "wasm"
        } else {
            "native"
        },
        tot.count[0],
        tot.secs[0],
        tot.count[1],
        tot.secs[1],
        tot.count[2],
        tot.secs[2],
        tot.failed_divs,
        tot.checksum
    );
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = args
        .get(1)
        .expect("usage: spike-replay TRACE [current|packed|both]");
    let which = args.get(2).map(String::as_str).unwrap_or("both");
    if which != "packed" {
        run(path, Current::new());
    }
    if which != "current" {
        run(path, Packed);
    }
}
