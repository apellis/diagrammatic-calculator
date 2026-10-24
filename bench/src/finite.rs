//! Phase 2 workloads: graded dimensions (B1, B3), cyclotomic quotients (B4), idempotent search
//! (B5), and Gram matrices (B6).

use super::{Bench, REPS, Timing, time_batch};
use diagcalc_core::builtin;
use diagcalc_core::context::{Context, ContextSpec};
use diagcalc_core::elab::CTerm;
use diagcalc_core::groebner::Limits;
use diagcalc_core::linalg::{self, Mat};
use diagcalc_core::perm::Perm;
use diagcalc_core::poly::Poly;
use diagcalc_core::quotient::Quotient;
use diagcalc_core::rep;
use diagcalc_core::ring::{PrimeField, Rationals, Ring};
use diagcalc_core::shapovalov::Shapovalov;
use diagcalc_core::solve;
use diagcalc_core::term::{Object, Word};
use diagcalc_core::toolkit;
use std::collections::{BTreeMap, HashMap};

type Q = Rationals;

fn context<R: Ring>(spec: &str, base: R) -> Context<R> {
    let spec = ContextSpec::parse(spec).expect("context");
    let pres = builtin::presentation(&spec.presentation).expect("presentation");
    Context::new(pres, spec, base).expect("context")
}

/// Coefficients of `Π_k 1/(1 − q^{d_k})` up to `q^top` (all `d_k > 0`).
fn dot_series(dots: &[i64], top: i64) -> Vec<u64> {
    let mut s = vec![0u64; (top + 1).max(0) as usize];
    if top < 0 {
        return s;
    }
    s[0] = 1;
    for &d in dots {
        for e in d as usize..s.len() {
            s[e] += s[e - d as usize];
        }
    }
    s
}

/// The KL graded dimension of `e(tgt) R(ν) e(src)` in degree `deg`:
/// `Σ_{w : w·src = tgt} q^{deg ψ_w} Π_a 1/(1 − q^{(i_a, i_a)})`, from the Cartan datum alone.
fn kl_dimension<R: Ring>(c: &Context<R>, src: &Object, tgt: Option<&Object>, deg: i64) -> u64 {
    // the nilHecke algebra has no Cartan datum: one colour with (i, i) = 2
    let form = |a: usize, b: usize| c.cartan.as_ref().map_or(2, |k| k.form(a, b));
    let n = src.len();
    let col = |a: usize| src[a].colour as usize;
    let dots: Vec<i64> = (0..n).map(|a| form(col(a), col(a))).collect();
    let mut total = 0;
    for w in Perm::all(n) {
        // (w·i)_{w(a)} = i_a
        let mut image = src.clone();
        for a in 0..n {
            image[w.0[a] as usize] = src[a];
        }
        if tgt.is_some_and(|t| *t != image) {
            continue;
        }
        let mut d = 0;
        for a in 0..n {
            for b in a + 1..n {
                if (w.0[a] as usize) > (w.0[b] as usize) {
                    d -= form(col(a), col(b));
                }
            }
        }
        let series = dot_series(&dots, deg - d);
        total += series.last().copied().unwrap_or(0);
    }
    total
}

impl Bench {
    /// B1, B3: graded dimensions of `R(ν) e(i)` and of every corner, by enumerating the basis,
    /// against the KL graded dimension formula.
    pub(crate) fn graded_dimensions(&mut self, id: &str, context: &str, span: i64) {
        let c = context_q(context);
        let ev = rep::evaluators(&c).expect("evaluator").remove(0);
        let mut counts = Vec::new();
        let secs = time_batch(&[()], REPS, |_| {
            counts.clear();
            for s in &c.objects {
                let lo = ev.min_degree(s).unwrap();
                for d in lo..=lo + span {
                    let b = ev.graded_basis(s, d).unwrap();
                    let mut by_target: BTreeMap<Object, u64> = BTreeMap::new();
                    for w in &b {
                        *by_target.entry(c.target(w)).or_default() += 1;
                    }
                    counts.push((s.clone(), d, b.len() as u64, by_target));
                }
            }
        });
        let ok = counts.iter().all(|(s, d, total, by)| {
            kl_dimension(&c, s, None, *d) == *total
                && c.objects
                    .iter()
                    .all(|t| kl_dimension(&c, s, Some(t), *d) == by.get(t).copied().unwrap_or(0))
        });
        let words: u64 = counts.iter().map(|x| x.2).sum();
        self.record(
            id,
            context,
            &format!("graded dimensions, {} degrees", span + 1),
            Timing {
                count: 1,
                secs,
                terms: words as f64,
                ok,
            },
        );
    }

    /// B4: the basis and graded dimensions of a cyclotomic quotient, against the q-Shapovalov
    /// form: `dim_q e(t) R^Λ(ν) e(s) = q^{D/2} ⟨F_{t'} v, F_{s'} v⟩`.
    pub(crate) fn cyclotomic(&mut self, context: &str) {
        let c = context_q(context);
        let mut dims = BTreeMap::new();
        let mut size = 0;
        let secs = time_batch(&[()], 1, |_| {
            let q = Quotient::from_context(&c, rep::evaluators(&c).unwrap().remove(0)).unwrap();
            size = toolkit::basis(&q).unwrap().len();
            dims.clear();
            for s in &c.objects {
                for t in &c.objects {
                    dims.insert(
                        (s.clone(), t.clone()),
                        q.graded_dimension(s, Some(t)).unwrap(),
                    );
                }
            }
        });
        let ok = shapovalov_agrees(&c, &dims);
        self.record(
            "B4",
            context,
            "cyclotomic quotient: basis, graded dims",
            Timing {
                count: 1,
                secs,
                terms: size as f64,
                ok,
            },
        );
    }

    /// B5: solving `e e = e` for `e = Σ c_k b_k` over the first `k` basis words of degree 0.
    pub(crate) fn idempotent_search(&mut self, context: &str, unknowns: usize) {
        let names: Vec<String> = (1..=unknowns).map(|k| format!("c{k}")).collect();
        let c = context_q(&format!("{context} scalars={}", names.join(",")));
        let ev = rep::evaluators(&c).expect("evaluator").remove(0);
        let mut words: Vec<Word> = Vec::new();
        for s in &c.objects {
            words.extend(ev.graded_basis(s, 0).unwrap());
        }
        words.truncate(unknowns);
        let mut e = CTerm::<Q>::zero();
        for (k, w) in words.iter().enumerate() {
            let ck = c.coeff.var(k);
            e = c.add(&e, &c.monomial_term(w, ck));
        }
        let watch = c.sub(&c.compose(&e, &e), &e);
        let mut out = serde_json::Value::Null;
        let secs = time_batch(&[()], REPS, |_| {
            let nf = ev.normal_form(&watch).unwrap();
            out = solve::solve(&c, &nf, &names, &Limits::default()).unwrap();
        });
        // Known idempotents: e = 0 and each idempotent basis word e(i) among the unknowns.
        let outcome = out["outcome"].as_str().unwrap_or("");
        let mut ok = matches!(outcome, "finite" | "family");
        if outcome == "finite" {
            let points = out["points"].as_array().unwrap();
            let zero: Vec<&str> = vec!["0"; unknowns];
            ok &= points.iter().any(|p| {
                p.as_array()
                    .unwrap()
                    .iter()
                    .map(|x| x.as_str().unwrap())
                    .eq(zero.iter().copied())
            });
            for (k, w) in words.iter().enumerate() {
                if w.layers.is_empty() {
                    let mut want = vec!["0"; unknowns];
                    want[k] = "1";
                    ok &= points.iter().any(|p| {
                        p.as_array()
                            .unwrap()
                            .iter()
                            .map(|x| x.as_str().unwrap())
                            .eq(want.iter().copied())
                    });
                }
            }
        }
        let steps = out["stats"]["steps"].as_u64().unwrap_or(0);
        self.record(
            "B5",
            context,
            &format!("solve e e = e, {unknowns} unknowns ({outcome})"),
            Timing {
                count: 1,
                secs,
                terms: steps as f64,
                ok,
            },
        );
    }

    /// B6: the Gram matrix `λ(x_a y_b)` of the first `n` basis words of a cyclotomic quotient
    /// against the last `n` (so degrees pair up), with `λ` the sum of the top-degree
    /// coefficients, and its rank over `Q` and over `F_p`.
    /// The pairing `A_d × A_{D−d} → ℚ`, `(x, y) ↦ λ(x ∘ y)`, of a cyclotomic quotient with trace
    /// degree `D`, for a generic functional `λ` on `A_D`; `d` is the degree whose piece has
    /// dimension closest to `n`. Graded symmetry makes it nondegenerate, so its rank over ℚ and
    /// over 𝔽_p is `dim A_d`.
    pub(crate) fn gram(&mut self, context: &str, n: usize) {
        let c = context_q(context);
        let q = Quotient::from_context(&c, rep::evaluators(&c).unwrap().remove(0)).unwrap();
        let sym = q
            .symmetric
            .expect("a cyclotomic quotient with a Cartan datum");
        let all = toolkit::basis(&q).unwrap();
        let of_degree = |d: i64| -> Vec<Word> {
            all.words
                .iter()
                .filter(|w| c.word_degree(w) == d)
                .cloned()
                .collect()
        };
        let d = (all.words.iter().map(|w| c.word_degree(w)).min().unwrap()..=sym / 2)
            .min_by_key(|&d| of_degree(d).len().abs_diff(n))
            .unwrap();
        // A pseudo-random functional with values in 1..=97.
        let mut seed: u64 = 0x9e37_79b9_7f4a_7c15;
        let functional: HashMap<Word, Poly<_>> = of_degree(sym)
            .into_iter()
            .map(|w| {
                seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
                let v = 1 + (seed >> 33) % 97;
                (w, c.coeff.constant(Rationals.int(v as i64)))
            })
            .collect();
        let term = |w: &Word| c.monomial_term(w, c.coeff.one());
        let xs: Vec<CTerm<Q>> = of_degree(d).iter().map(term).collect();
        let ys: Vec<CTerm<Q>> = of_degree(sym - d).iter().map(term).collect();
        let dim = xs.len();
        let mut g = None;
        let secs = time_batch(&[()], 1, |_| {
            g = Some(toolkit::gram(&q, &xs, &ys, &functional).unwrap());
        });
        let g = g.unwrap();
        let size = format!(
            "{} x {} (degrees {d} and {})",
            g.nrows(),
            g.ncols(),
            sym - d
        );
        self.record(
            "B6",
            context,
            &format!("Gram matrix {size}"),
            Timing {
                count: 1,
                secs,
                terms: (g.nrows() * g.ncols()) as f64,
                ok: xs.len() == ys.len(),
            },
        );
        // the entries are rationals: rank over Q and over F_p
        let rq = g.map(|x| c.coeff.constant_term(x));
        let mut rank_q = 0;
        let secs = time_batch(&[()], REPS, |_| {
            rank_q = linalg::rank(&Rationals, &rq).unwrap()
        });
        let p = PrimeField::new(1_000_003).unwrap();
        let rp: Mat<_> = rq.map(|x| {
            let (num, den) = (x.numer().to_string(), x.denom().to_string());
            let n = p.int(num.parse::<i64>().unwrap());
            let d = p.int(den.parse::<i64>().unwrap());
            p.div_exact(&n, &d).unwrap()
        });
        let mut rank_p = 0;
        let secs_p = time_batch(&[()], REPS, |_| rank_p = linalg::rank(&p, &rp).unwrap());
        let ok = rank_q == rank_p && rank_q == dim;
        self.record(
            "B6",
            context,
            &format!("rank over QQ of {size} (rank {rank_q})"),
            Timing {
                count: 1,
                secs,
                terms: rank_q as f64,
                ok,
            },
        );
        self.record(
            "B6",
            context,
            &format!("rank over GF(1000003) of {size}"),
            Timing {
                count: 1,
                secs: secs_p,
                terms: rank_p as f64,
                ok,
            },
        );
    }
}

fn context_q(spec: &str) -> Context<Q> {
    context(&format!("{spec} ring=QQ"), Rationals)
}

/// Graded dimensions of every corner against the q-Shapovalov form.
fn shapovalov_agrees(
    c: &Context<Q>,
    dims: &BTreeMap<(Object, Object), BTreeMap<i64, usize>>,
) -> bool {
    let cartan = c.cartan.clone().unwrap();
    let lam: Vec<i64> = c
        .parse_weight(c.spec.lambda.as_ref().unwrap())
        .unwrap()
        .iter()
        .map(|&x| x as i64)
        .collect();
    let nu = c.weight.clone().unwrap();
    let r = nu.len();
    let d_top: i64 = 2
        * (0..r)
            .map(|i| lam[i] * cartan.d[i] * nu[i] as i64)
            .sum::<i64>()
        - (0..r)
            .flat_map(|i| (0..r).map(move |j| (i, j)))
            .map(|(i, j)| nu[i] as i64 * nu[j] as i64 * cartan.form(i, j))
            .sum::<i64>();
    let mut sh = Shapovalov::new(&cartan, &lam);
    dims.iter().all(|((s, t), got)| {
        let rev = |o: &Object| o.iter().rev().map(|l| l.colour).collect::<Vec<u16>>();
        let form = sh.form(&rev(t), &rev(s));
        let want: BTreeMap<i64, usize> = form
            .terms
            .iter()
            .map(|(m, k)| {
                (
                    m.degree() + d_top / 2,
                    k.to_string().parse::<usize>().unwrap(),
                )
            })
            .collect();
        *got == want
    })
}
