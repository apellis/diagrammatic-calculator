//! The evaluation suite (specification, "Performance targets and the evaluation suite"):
//! workloads B1–B10. Each workload checks its answers and times the engine; the results are
//! printed as a table and written as JSON (`diagcalc-bench/1`).
//!
//! ```text
//! diagcalc-bench [--quick] [--only B1,B3] [--out FILE] [--label TEXT] [--trace FILE]
//! ```
//!
//! `--trace` (built with the `trace` feature) logs the polynomial operations of the timed work,
//! one pass of each batch, for replay by the arithmetic spike (`bench/spike/`).
//!
//! Random inputs come from a fixed seed, so every run times the same terms.

mod finite;

use diagcalc_core::builtin;
use diagcalc_core::context::{Context, ContextSpec};
use diagcalc_core::elab::CTerm;
use diagcalc_core::poly::Monomial;
use diagcalc_core::rep::{self, Evaluator};
use diagcalc_core::rewrite;
use diagcalc_core::ring::{Integers, Ring};
use diagcalc_core::term::Letter;
use serde_json::{Value, json};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::RefCell;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

/// Counts live heap bytes and their peak, for B10.
struct Counting;

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(l) };
        if !p.is_null() {
            let now = LIVE.fetch_add(l.size(), Ordering::Relaxed) + l.size();
            PEAK.fetch_max(now, Ordering::Relaxed);
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) };
        LIVE.fetch_sub(l.size(), Ordering::Relaxed);
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, new: usize) -> *mut u8 {
        let q = unsafe { System.realloc(p, l, new) };
        if !q.is_null() {
            if new > l.size() {
                let now = LIVE.fetch_add(new - l.size(), Ordering::Relaxed) + new - l.size();
                PEAK.fetch_max(now, Ordering::Relaxed);
            } else {
                LIVE.fetch_sub(l.size() - new, Ordering::Relaxed);
            }
        }
        q
    }
}

#[global_allocator]
static ALLOC: Counting = Counting;

/// Starts a new peak-memory window; returns the live bytes at its start.
fn reset_peak() -> usize {
    let live = LIVE.load(Ordering::Relaxed);
    PEAK.store(live, Ordering::Relaxed);
    live
}

/// Bytes allocated above `base` at the peak of the current window.
fn peak_since(base: usize) -> usize {
    PEAK.load(Ordering::Relaxed).saturating_sub(base)
}

thread_local! {
    /// The trace of the timed work, when `--trace` is given.
    static TRACE: RefCell<Option<String>> = const { RefCell::new(None) };
}

type R = Integers;

/// xorshift64*: a small deterministic generator, so that runs are comparable.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// One timed batch: how many operations, the fastest pass, the mean number of terms of the
/// answers, and whether the checks passed.
struct Timing {
    count: usize,
    secs: f64,
    terms: f64,
    ok: bool,
}

struct Options {
    quick: bool,
    only: Option<Vec<String>>,
    out: Option<String>,
    label: Option<String>,
    trace: Option<String>,
}

struct Bench {
    opts: Options,
    results: Vec<Value>,
    failed: usize,
    /// Live bytes when the current workload's peak window started.
    mem_base: usize,
}

/// One context with its first declared evaluator.
struct Setup {
    ctx: Context<R>,
}

impl Setup {
    fn new(context: &str) -> Setup {
        let spec = ContextSpec::parse(&format!("{context} ring=ZZ")).expect("context");
        let pres = builtin::presentation(&spec.presentation).expect("presentation");
        Setup {
            ctx: Context::new(pres, spec, Integers).expect("context"),
        }
    }
    fn eval(&self) -> Box<dyn Evaluator<R> + '_> {
        rep::evaluators(&self.ctx).expect("evaluator").remove(0)
    }
    fn term(&self, s: &str) -> CTerm<R> {
        self.ctx
            .parse_term(s)
            .unwrap_or_else(|e| panic!("{s}: {e}"))
    }
}

/// The memory ceiling of the smallest device class (a phone browser tab), for B10.
const PHONE_CEILING: u64 = 256 << 20;

/// Passes over each batch; the fastest is kept.
const REPS: usize = 3;

/// Times `f` over every input, `reps` times, and keeps the fastest pass.
fn time_batch<T>(inputs: &[T], reps: usize, mut f: impl FnMut(&T)) -> f64 {
    #[cfg(feature = "trace")]
    if TRACE.with(|t| t.borrow().is_some()) {
        diagcalc_core::poly::trace::start();
        inputs.iter().for_each(&mut f);
        let log = diagcalc_core::poly::trace::take().unwrap_or_default();
        TRACE.with(|t| t.borrow_mut().as_mut().unwrap().push_str(&log));
    }
    let mut best = f64::INFINITY;
    for _ in 0..reps {
        let start = Instant::now();
        for x in inputs {
            f(x);
        }
        best = best.min(start.elapsed().as_secs_f64());
    }
    best
}

/// A random word on `n` strands, bottom layer first, updating `obj` to its target. Crossings
/// mostly lengthen the permutation (a quarter of them need not), and no crossing directly repeats
/// the one below it, so that most words are nonzero even where `psi_k² = 0`.
fn random_layers(rng: &mut Rng, n: usize, len: usize, obj: &mut [char]) -> Vec<String> {
    // strand[k]: the starting position of the strand now at position k
    let mut strand: Vec<usize> = (0..n).collect();
    let mut last = None;
    (0..len)
        .map(|_| {
            let up: Vec<usize> = (0..n - 1)
                .filter(|&k| strand[k] < strand[k + 1] && last != Some(k))
                .collect();
            let any: Vec<usize> = (0..n - 1).filter(|&k| last != Some(k)).collect();
            let choices = if rng.below(4) == 0 { &any } else { &up };
            if rng.below(3) == 0 || choices.is_empty() {
                last = None;
                format!("x_{}", rng.below(n) + 1)
            } else {
                let k = choices[rng.below(choices.len())];
                strand.swap(k, k + 1);
                obj.swap(k, k + 1);
                last = Some(k);
                format!("psi_{}", k + 1)
            }
        })
        .collect()
}

/// A random word for straightening: its running degree stays within ±6 where possible, and a
/// crossing of two strands of one colour that would shorten the permutation comes only straight
/// after a dot on one of them (as in `psi_1 x_1 psi_1 = psi_1`), so long words seldom vanish. Returns the layers, bottom first, and
/// leaves the target object in `obj`.
fn balanced_layers(
    ctx: &Context<R>,
    rng: &mut Rng,
    len: usize,
    obj: &mut Vec<Letter>,
) -> Vec<String> {
    let dot = ctx.pres.generator("x").expect("x");
    let psi = ctx.pres.generator("psi").expect("psi");
    let n = obj.len();
    let mut strand: Vec<usize> = (0..n).collect();
    let mut deg = 0i64;
    let mut last_dot = None;
    let mut out = Vec::with_capacity(len);
    for _ in 0..len {
        let mut cands = Vec::new();
        for k in 0..n {
            if let Some(l) = ctx.match_layer(dot, obj, k) {
                cands.push((l, false));
            }
            if k + 1 >= n {
                continue;
            }
            let up = strand[k] < strand[k + 1];
            let mixed = obj[k].colour != obj[k + 1].colour;
            let after_dot = matches!(last_dot, Some(j) if j == k || j == k + 1);
            if (up || mixed || after_dot)
                && let Some(l) = ctx.match_layer(psi, obj, k)
            {
                cands.push((l, true));
            }
        }
        let within: Vec<usize> = (0..cands.len())
            .filter(|&c| (-6..=6).contains(&(deg + ctx.layer_degree(&cands[c].0))))
            .collect();
        let pick = if within.is_empty() {
            rng.below(cands.len())
        } else {
            within[rng.below(within.len())]
        };
        let (l, crossing) = cands.swap_remove(pick);
        deg += ctx.layer_degree(&l);
        let k = l.pos as usize;
        if crossing {
            // psi_k x psi_k on strands of one colour keeps the leading permutation
            if strand[k] < strand[k + 1] || obj[k].colour != obj[k + 1].colour {
                strand.swap(k, k + 1);
            }
            last_dot = None;
        } else {
            last_dot = Some(k);
        }
        *obj = ctx.apply(&l, obj);
        out.push(format!("{}_{}", if crossing { "psi" } else { "x" }, k + 1));
    }
    out
}

fn text(layers: &[String], idem: Option<&[char]>) -> String {
    let mut s: Vec<String> = layers.iter().rev().cloned().collect();
    if let Some(o) = idem {
        s.push(format!("e({})", o.iter().collect::<String>()));
    }
    s.join(" ")
}

/// `psi_{w_0}` as a reduced word, top to bottom.
fn longest_word(n: usize) -> String {
    let mut w = Vec::new();
    for k in 1..n {
        for j in (1..=k).rev() {
            w.push(format!("psi_{j}"));
        }
    }
    w.join(" ")
}

/// `x^δ = x_1^{n-1} x_2^{n-2} ⋯ x_{n-1}`.
fn staircase(n: usize) -> String {
    (1..n)
        .map(|k| match n - k {
            1 => format!("x_{k}"),
            e => format!("x_{k}^{e}"),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn mean_terms(ev: &dyn Evaluator<R>, ts: &[CTerm<R>]) -> f64 {
    let total: usize = ts.iter().map(|t| ev.normal_form(t).unwrap().len()).sum();
    total as f64 / ts.len().max(1) as f64
}

/// Whether a term and its normal form act alike on a few monomials at every given source.
fn agrees(s: &Setup, ev: &dyn Evaluator<R>, t: &CTerm<R>, sources: &[usize]) -> bool {
    let nf = ev.normal_form(t).unwrap();
    for &o in sources {
        let obj = &s.ctx.objects[o];
        let n = obj.len();
        let ring = ev.poly_ring(n);
        for k in 0..=n.min(3) {
            let mut m = vec![0i32; ring.nvars()];
            if k > 0 {
                m[k - 1] = 1;
                m[0] += 1;
            }
            let f = ring.monomial(Monomial::from(m), ring.base.one());
            if ev.act(t, obj, &f).unwrap() != ev.act(&nf, obj, &f).unwrap() {
                return false;
            }
        }
    }
    true
}

impl Bench {
    fn wants(&self, id: &str) -> bool {
        self.opts
            .only
            .as_ref()
            .is_none_or(|l| l.iter().any(|x| x == id))
    }

    /// Records one timing; `terms` is the mean number of terms of the answers.
    fn record(&mut self, id: &str, context: &str, op: &str, t: Timing) {
        let Timing {
            count,
            secs,
            terms,
            ok,
        } = t;
        if !ok {
            self.failed += 1;
        }
        let peak = peak_since(self.mem_base);
        self.mem_base = reset_peak();
        eprintln!(
            "{id:<3} {context:<24} {op:<34} {count:>3} × {:>10.1} µs {terms:>7.1} terms {:>8.1} MB  {}",
            secs / count as f64 * 1e6,
            peak as f64 / 1e6,
            if ok { "ok" } else { "CHECK FAILED" }
        );
        self.results.push(json!({
            "id": id,
            "context": context,
            "op": op,
            "count": count,
            "seconds": secs,
            "per_op_us": secs / count as f64 * 1e6,
            "mean_terms": terms,
            "peak_bytes": peak,
            "check": ok,
        }));
    }

    /// Simplification of random words and products of normal forms, with an agreement check.
    fn random_words(&mut self, id: &str, context: &str, seed: u64, words: usize, len: usize) {
        let s = Setup::new(context);
        let ev = s.eval();
        let mut rng = Rng(seed);
        let coloured = s.ctx.objects.len() > 1;
        let mut singles = Vec::new();
        let mut pairs = Vec::new();
        let mut sources = Vec::new();
        for _ in 0..words {
            let o = rng.below(s.ctx.objects.len());
            let start: Vec<char> = s.ctx.fmt_object(&s.ctx.objects[o]).chars().collect();
            let n = start.len();
            let mut obj = start.clone();
            let bottom = random_layers(&mut rng, n, len, &mut obj);
            let mid = obj.clone();
            let top = random_layers(&mut rng, n, len, &mut obj);
            let idem = |x: &[char]| coloured.then(|| x.to_vec());
            let b = s.term(&text(&bottom, idem(&start).as_deref()));
            let a = s.term(&text(&top, idem(&mid).as_deref()));
            singles.push(b.clone());
            pairs.push((ev.normal_form(&a).unwrap(), ev.normal_form(&b).unwrap()));
            sources.push(o);
        }
        let ok = singles
            .iter()
            .zip(&sources)
            .take(3)
            .all(|(t, &o)| agrees(&s, ev.as_ref(), t, &[o]));
        let secs = time_batch(&singles, REPS, |t| {
            ev.normal_form(t).unwrap();
        });
        let terms = mean_terms(ev.as_ref(), &singles);
        self.record(
            id,
            context,
            &format!("simplify random word, {len} layers"),
            Timing {
                count: words,
                secs,
                terms,
                ok,
            },
        );
        let products: Vec<CTerm<R>> = pairs.iter().map(|(a, b)| s.ctx.compose(a, b)).collect();
        let ok = products
            .iter()
            .zip(&sources)
            .take(3)
            .all(|(t, &o)| agrees(&s, ev.as_ref(), t, &[o]));
        let secs = time_batch(&products, REPS, |t| {
            ev.normal_form(t).unwrap();
        });
        let terms = mean_terms(ev.as_ref(), &products);
        self.record(
            id,
            context,
            "product of two normal forms",
            Timing {
                count: words,
                secs,
                terms,
                ok,
            },
        );
    }

    /// The watch `e² − e` for `e = c x^δ psi_{w_0}`, where `psi_{w_0} x^δ psi_{w_0} = c psi_{w_0}`
    /// with `c = ±1` (`c = 1` in the even case).
    fn idempotent(&mut self, id: &str, context: &str, n: usize) {
        let s = Setup::new(context);
        let ev = s.eval();
        let w0 = longest_word(n);
        let d = staircase(n);
        let sandwich = s.term(&format!("{w0} {d} {w0}"));
        let c = [1, -1].into_iter().find(|c| {
            let rhs = s.term(&format!("{c} {w0}"));
            ev.is_zero(&s.ctx.sub(&sandwich, &rhs)).unwrap()
        });
        let ok = c.is_some();
        let e = s.term(&format!("{} {d} {w0}", c.unwrap_or(1)));
        let watch = s.ctx.sub(&s.ctx.compose(&e, &e), &e);
        let mut zero = false;
        let secs = time_batch(&[()], REPS, |_| {
            zero = ev.is_zero(&watch).unwrap();
        });
        let terms = s.ctx.compose(&e, &e).len() as f64;
        self.record(
            id,
            context,
            "watch e^2 - e, e = x^d psi_w0",
            Timing {
                count: 1,
                secs,
                terms,
                ok: ok && zero,
            },
        );
    }

    /// Koszul signs: distant odd generators anticommute.
    fn signs(&mut self, id: &str, context: &str) {
        let s = Setup::new(context);
        let ev = s.eval();
        let checks = [
            "psi_1 psi_3 + psi_3 psi_1",
            "x_1 x_3 + x_3 x_1",
            "x_1 psi_3 + psi_3 x_1",
            "x_1 x_2 + x_2 x_1",
        ];
        let terms: Vec<CTerm<R>> = checks.iter().map(|c| s.term(c)).collect();
        let ok = terms.iter().all(|t| ev.is_zero(t).unwrap())
            && !ev.is_zero(&s.term("x_1 x_2 - x_2 x_1")).unwrap();
        let secs = time_batch(&terms, REPS, |t| {
            ev.is_zero(t).unwrap();
        });
        self.record(
            id,
            context,
            "Koszul sign checks",
            Timing {
                count: terms.len(),
                secs,
                terms: 0.0,
                ok,
            },
        );
    }

    /// B7: drawing sums of distinct basis diagrams as SVG and TikZ.
    fn render(&mut self, context: &str, sizes: &[usize]) {
        let s = Setup::new(context);
        let ev = s.eval();
        // distinct basis words, low degrees first, from every object
        let mut words = Vec::new();
        let mut d = -20;
        while words.len() < *sizes.iter().max().unwrap() && d < 60 {
            for o in &s.ctx.objects {
                words.extend(ev.graded_basis(o, d).unwrap());
            }
            d += 1;
        }
        for &n in sizes {
            let mut t = CTerm::<R>::zero();
            for (k, w) in words.iter().take(n).enumerate() {
                let c = s.ctx.coeff.int(k as i64 % 7 + 1);
                t = s.ctx.add(&t, &s.ctx.monomial_term(w, c));
            }
            let ok = !t.is_zero();
            for (fmt, f) in [
                (
                    "svg",
                    diagcalc_core::render::svg::<R> as fn(&Context<R>, &CTerm<R>) -> String,
                ),
                ("tikz", diagcalc_core::render::tikz::<R>),
            ] {
                let mut out = 0;
                let secs = time_batch(&[()], REPS, |_| out = f(&s.ctx, &t).len());
                self.record(
                    "B7",
                    context,
                    &format!("draw a sum of {} diagrams, {fmt}", t.len()),
                    Timing {
                        count: 1,
                        secs,
                        terms: t.len() as f64,
                        ok: ok && out > 0,
                    },
                );
            }
        }
    }

    /// B8: what one interaction costs the engine: re-evaluating an edited cell, sliding a dot
    /// through a crossing, and checking a relation. Each includes parsing and elaboration.
    fn interactive(&mut self, context: &str, cases: &[(&str, &str, &str, bool)]) {
        let s = Setup::new(context);
        let session = diagcalc_core::ops::Session::new(&s.ctx, None).expect("session");
        for &(op, a, b, expect) in cases {
            let mut holds = None;
            let secs = time_batch(&[()], REPS, |_| {
                let ta = session.parse(a).unwrap();
                let tb = session.parse(b).unwrap();
                let v = session.check("equal", &[ta, tb]).unwrap();
                holds = v["holds"].as_bool();
            });
            self.record(
                "B8",
                context,
                op,
                Timing {
                    count: 1,
                    secs,
                    terms: 0.0,
                    ok: holds == Some(expect),
                },
            );
        }
    }

    /// B9: straightening long random words to normal form by rewriting, each result checked
    /// against the representation's normal form; the representation's own time is recorded
    /// alongside.
    fn long_words(&mut self, context: &str, lens: &[usize], words: usize) {
        let s = Setup::new(context);
        let ev = s.eval();
        let rules = rewrite::rules(&s.ctx).expect("rules");
        let layout = rep::basis_layout(&s.ctx, &s.ctx.pres.representations[0]).expect("layout");
        let coloured = s.ctx.objects.len() > 1;
        for &len in lens {
            let mut rng = Rng(0xb9 + len as u64);
            let mut ts = Vec::new();
            let mut sources = Vec::new();
            for _ in 0..words {
                let o = rng.below(s.ctx.objects.len());
                let start: Vec<char> = s.ctx.fmt_object(&s.ctx.objects[o]).chars().collect();
                let mut obj = s.ctx.objects[o].clone();
                let layers = balanced_layers(&s.ctx, &mut rng, len, &mut obj);
                ts.push(s.term(&text(&layers, coloured.then_some(&start[..]))));
                sources.push(o);
            }
            let ok = ts
                .iter()
                .zip(&sources)
                .take(2)
                .all(|(t, &o)| agrees(&s, ev.as_ref(), t, &[o]));
            let secs = time_batch(&ts, 1, |t| {
                ev.normal_form(t).unwrap();
            });
            let terms = mean_terms(ev.as_ref(), &ts);
            self.record(
                "B9",
                context,
                &format!("normal form by the representation, {len} layers"),
                Timing {
                    count: words,
                    secs,
                    terms,
                    ok,
                },
            );
            let normal = |t: &CTerm<R>| {
                rewrite::normal_form(&s.ctx, &rules, &layout, t, 10_000_000).expect("rewriting")
            };
            // One timed pass, checked afterwards: the long words take seconds each.
            let start = Instant::now();
            let results: Vec<(CTerm<R>, usize)> = ts.iter().map(normal).collect();
            let secs = start.elapsed().as_secs_f64();
            let steps: usize = results.iter().map(|r| r.1).sum();
            let ok = ts
                .iter()
                .zip(&results)
                .all(|(t, (nf, _))| *nf == ev.normal_form(t).unwrap());
            self.record(
                "B9",
                context,
                &format!("normal form by rewriting, {len} layers"),
                Timing {
                    count: words,
                    secs,
                    terms,
                    ok,
                },
            );
            if let Some(r) = self.results.last_mut() {
                r["mean_steps"] = json!(steps / words);
            }
            // Keeping the chain (every step, for provenance and export) costs memory in
            // proportion to steps times layers; it is timed where that stays small.
            if len <= 50 {
                let ok = ts.iter().all(|t| {
                    let chain = rewrite::straighten(&s.ctx, &rules, &layout, t, 10_000_000)
                        .expect("straighten");
                    chain.end == ev.normal_form(t).unwrap()
                });
                let secs = time_batch(&ts, 1, |t| {
                    rewrite::straighten(&s.ctx, &rules, &layout, t, 10_000_000)
                        .expect("straighten");
                });
                self.record(
                    "B9",
                    context,
                    &format!("straighten with the chain kept, {len} layers"),
                    Timing {
                        count: words,
                        secs,
                        terms,
                        ok,
                    },
                );
            }
        }
    }

    fn run(&mut self) {
        let max_n = if self.opts.quick { 5 } else { 6 };
        let words = if self.opts.quick { 10 } else { 20 };
        if self.wants("B1") {
            for n in 3..=max_n {
                let c = format!("nilhecke n={n}");
                self.random_words("B1", &c, 0xb1 + n as u64, words, 2 * n + 2);
                self.idempotent("B1", &c, n);
                self.graded_dimensions("B1", &c, 12);
            }
        }
        if self.wants("B2") {
            for n in 3..=max_n {
                let c = format!("odd-nilhecke n={n}");
                self.random_words("B2", &c, 0xb2 + n as u64, words, 2 * n + 2);
                self.idempotent("B2", &c, n);
                if n >= 4 {
                    self.signs("B2", &c);
                }
            }
        }
        if self.wants("B3") {
            for (c, len) in [
                ("klr type=A2 nu=2i+j", 8),
                ("klr type=A3 nu=i+2j+k", 10),
                ("klr type=B2 nu=i+j", 6),
                ("klr type=B2 nu=2i+j", 8),
                ("klr type=G2 nu=i+j", 6),
                ("klr type=G2 nu=2i+j", 8),
            ] {
                self.random_words("B3", c, 0xb3 + len as u64, words, len);
                self.graded_dimensions("B3", c, 8);
            }
        }
        if self.wants("B4") {
            let mut cases = vec![
                "klr type=A2 nu=2i+j Lambda=2i",
                "klr type=A2 nu=2i+j Lambda=i+j",
                "klr type=A3 nu=i+2j+k Lambda=j+k",
                "klr type=B2 nu=i+2j Lambda=i",
                "klr type=B2 nu=2i+2j Lambda=i",
            ];
            if !self.opts.quick {
                cases.push("klr type=A2 nu=2i+2j Lambda=2i+j");
            }
            for c in cases {
                self.cyclotomic(c);
            }
        }
        if self.wants("B5") {
            for k in [2, 4, 8] {
                self.idempotent_search("klr type=A2 nu=2i+j", k);
            }
        }
        if self.wants("B6") {
            self.gram("klr type=A2 nu=2i+2j Lambda=2i+j", 50);
            if !self.opts.quick {
                self.gram("klr type=A2 nu=2i+2j Lambda=2i+2j", 200);
            }
        }
        if self.wants("B7") {
            let sizes: &[usize] = if self.opts.quick {
                &[10, 100]
            } else {
                &[10, 100, 1000]
            };
            self.render("klr type=A2 nu=2i+j", sizes);
        }
        if self.wants("B8") {
            self.interactive(
                "nilhecke n=3",
                &[
                    (
                        "edit a watch: (x_1 psi_1)^2 psi_2",
                        "x_1 psi_1 x_1 psi_1 psi_2",
                        "x_1 psi_1 psi_2 x_1 + psi_1 psi_2",
                        false,
                    ),
                    (
                        "slide a dot through a crossing",
                        "x_2 psi_1",
                        "psi_1 x_1 + 1",
                        false,
                    ),
                    (
                        "apply the braid relation",
                        "psi_1 psi_2 psi_1",
                        "psi_2 psi_1 psi_2",
                        true,
                    ),
                ],
            );
            self.interactive(
                "klr type=A2 nu=2i+j",
                &[
                    (
                        "edit a watch: psi_2 psi_1 x_1 psi_2",
                        "psi_2 psi_1 x_1 psi_2 e(iij)",
                        "psi_2 psi_1 psi_2 x_1 e(iij)",
                        true,
                    ),
                    (
                        "slide a dot through a crossing",
                        "x_2 psi_1 e(iij)",
                        "psi_1 x_1 e(iij) - e(iij)",
                        true,
                    ),
                    (
                        "apply the braid relation with correction",
                        "psi_1 psi_2 psi_1 e(iji)",
                        "psi_2 psi_1 psi_2 e(iji)",
                        false,
                    ),
                ],
            );
        }
        if self.wants("B9") {
            let (lens, words): (&[usize], usize) = if self.opts.quick {
                (&[10, 50, 100], 3)
            } else {
                (&[10, 50, 100, 200], 3)
            };
            self.long_words("nilhecke n=4", lens, words);
            self.long_words("klr type=A2 nu=2i+j", lens, words);
        }
        if self.wants("B10") {
            self.memory();
        }
    }

    /// B10: the largest peak of heap memory over the workloads run so far, against the smallest
    /// device class's ceiling (a phone browser tab).
    fn memory(&mut self) {
        let (peak, worst) = self
            .results
            .iter()
            .map(|r| {
                (
                    r["peak_bytes"].as_u64().unwrap_or(0),
                    format!("{} {} {}", r["id"], r["context"], r["op"]).replace('"', ""),
                )
            })
            .max()
            .unwrap_or((0, "nothing run".into()));
        eprintln!("B10 largest peak {:.1} MB: {worst}", peak as f64 / 1e6);
        self.record(
            "B10",
            "all workloads",
            "peak heap of the largest workload",
            Timing {
                count: 1,
                secs: 0.0,
                terms: peak as f64,
                ok: peak < PHONE_CEILING,
            },
        );
    }
}

fn parse_args() -> Result<Options, String> {
    let mut opts = Options {
        quick: false,
        only: None,
        out: None,
        label: None,
        trace: None,
    };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        let mut value = || it.next().ok_or(format!("{a} needs a value"));
        match a.as_str() {
            "--quick" => opts.quick = true,
            "--only" => opts.only = Some(value()?.split(',').map(String::from).collect()),
            "--out" => opts.out = Some(value()?),
            "--label" => opts.label = Some(value()?),
            "--trace" if cfg!(feature = "trace") => opts.trace = Some(value()?),
            "--trace" => return Err("--trace needs the trace feature".into()),
            _ => return Err(format!("unknown argument {a}")),
        }
    }
    Ok(opts)
}

fn main() {
    let opts = match parse_args() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("diagcalc-bench: {e}");
            std::process::exit(2);
        }
    };
    let mut bench = Bench {
        opts,
        results: Vec::new(),
        failed: 0,
        mem_base: reset_peak(),
    };
    if bench.opts.trace.is_some() {
        TRACE.with(|t| *t.borrow_mut() = Some(String::new()));
    }
    let start = Instant::now();
    bench.run();
    if let Some(path) = &bench.opts.trace {
        let log = TRACE.with(|t| t.borrow_mut().take()).unwrap_or_default();
        std::fs::write(path, log).expect("write trace");
    }
    let total = start.elapsed().as_secs_f64();
    eprintln!("total {total:.2} s, {} check(s) failed", bench.failed);
    let report = json!({
        "format": "diagcalc-bench/1",
        "label": bench.opts.label,
        "platform": if cfg!(target_family = "wasm") { "wasm" } else { "native" },
        "quick": bench.opts.quick,
        "unix_time": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        "total_seconds": total,
        "results": bench.results,
    });
    let text = serde_json::to_string_pretty(&report).unwrap();
    match &bench.opts.out {
        Some(path) => std::fs::write(path, text + "\n").expect("write results"),
        None => println!("{text}"),
    }
    if bench.failed > 0 {
        std::process::exit(1);
    }
}
