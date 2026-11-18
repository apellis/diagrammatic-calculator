//! The cyclotomic quotient `T^λ` of a weighted KLR algebra against the tensor-product
//! Shapovalov form (B. Webster, *Knot invariants and higher representation theory*,
//! arXiv:1309.3796v2, §4). For loadings `B`, `B'` with every red strand at weight `0` from every
//! black one, `dim_q e_{B'} T^λ e_B = ⟨u_B, u_{B'}⟩` exactly, in the doubled grading, where:
//!
//! - `u_B ∈ V(λ_1) ⊗ ⋯ ⊗ V(λ_ℓ)` is built from the left: the black strands right of red strand
//!   `j` (and left of red strand `j + 1`) act on `V(λ_1) ⊗ ⋯ ⊗ V(λ_j)` by `F_i`, leftmost strand
//!   first, through the coproduct `Δ(F_i) = F_i ⊗ K_i + 1 ⊗ F_i`;
//! - the form is the product over the factors of `q^{D_m/2}` times the Shapovalov form of
//!   `V(λ_m)`, where `D_m = 2(λ_m, ν_m) − (ν_m, ν_m)` and `ν_m` is the weight lowered in factor
//!   `m` (`q^{D_m/2}` is the shift making the cyclotomic KLR algebra `R^{λ_m}(ν_m)` symmetric).

use diagcalc_core::context::ContextSpec;
use diagcalc_core::num::Int;
use diagcalc_core::poly::{Monomial, Poly, PolyRing};
use diagcalc_core::rep::klrw::Klrw;
use diagcalc_core::rep::weighted::{Loading, Q};
use diagcalc_core::ring::{Integers, Rationals, Ring};
use diagcalc_core::shapovalov::{Shapovalov, laurent_ring};
use std::collections::BTreeMap;

type L = Poly<Int>;

struct Setup {
    k: Klrw<Rationals>,
    lambdas: Vec<Vec<i64>>,
    reds: Vec<Q>,
}

fn setup(spec: &str) -> Setup {
    let k = Klrw::new(&ContextSpec::parse(spec).unwrap(), Rationals).unwrap();
    let lambdas = k
        .data
        .reds
        .iter()
        .map(|r| r.exps.iter().map(|&x| x as i64).collect())
        .collect();
    let reds = k.data.reds.iter().map(|r| r.pos.clone()).collect();
    Setup { k, lambdas, reds }
}

fn qpow(r: &PolyRing<Integers>, e: i64) -> L {
    r.monomial(Monomial::from(vec![e as i32]), Int::from(1))
}

/// Every placement of the colours into the regions left of, between, and right of the red
/// strands: `regions[r]` lists the black strands right of red strand `r` (region 0: left of all).
fn placements(colours: &[u16], nregions: usize) -> Vec<Vec<Vec<u16>>> {
    fn go(
        left: &mut BTreeMap<u16, usize>,
        n: usize,
        nreg: usize,
        cur: &mut Vec<Vec<u16>>,
        out: &mut Vec<Vec<Vec<u16>>>,
    ) {
        let placed: usize = cur.iter().map(Vec::len).sum();
        if placed == n {
            out.push(cur.clone());
            return;
        }
        let keys: Vec<u16> = left.keys().copied().collect();
        for c in keys {
            if left[&c] == 0 {
                continue;
            }
            for r in 0..nreg {
                // add c to region r, keeping regions after r empty so far
                if cur[r + 1..].iter().any(|v| !v.is_empty()) {
                    continue;
                }
                *left.get_mut(&c).unwrap() -= 1;
                cur[r].push(c);
                go(left, n, nreg, cur, out);
                cur[r].pop();
                *left.get_mut(&c).unwrap() += 1;
            }
        }
    }
    let mut left = BTreeMap::new();
    for &c in colours {
        *left.entry(c).or_insert(0) += 1;
    }
    let mut out = Vec::new();
    go(
        &mut left,
        colours.len(),
        nregions,
        &mut vec![Vec::new(); nregions],
        &mut out,
    );
    out
}

fn loading(s: &Setup, regions: &[Vec<u16>]) -> Loading {
    let mut pts = Vec::new();
    for (r, strands) in regions.iter().enumerate() {
        let start = if r == 0 {
            s.reds[0].clone() - Q::from_integer((strands.len() as i64 + 1).into())
        } else {
            s.reds[r - 1].clone()
        };
        for (m, &c) in strands.iter().enumerate() {
            pts.push((c, start.clone() + Q::from_integer((m as i64 + 1).into())));
        }
    }
    Loading(pts)
}

/// `u_B` as a combination of pure tensors of words (`F_{w_1} ⋯ F_{w_m} v`, `w_m` first).
fn vector(s: &Setup, regions: &[Vec<u16>], r: &PolyRing<Integers>) -> BTreeMap<Vec<Vec<u16>>, L> {
    let cartan = s.k.klr.cartan.as_ref().unwrap();
    let mut v: BTreeMap<Vec<Vec<u16>>, L> = BTreeMap::new();
    if !regions[0].is_empty() {
        return v;
    }
    v.insert(Vec::new(), r.one());
    let k_exp = |j: usize, i: usize, word: &[u16]| -> i64 {
        let wt = s.lambdas[j][i] - word.iter().map(|&c| cartan.a[i][c as usize]).sum::<i64>();
        cartan.d[i] * wt
    };
    for j in 0..s.lambdas.len() {
        v = v
            .into_iter()
            .map(|(mut t, c)| {
                t.push(Vec::new());
                (t, c)
            })
            .collect();
        for &i in &regions[j + 1] {
            let mut next: BTreeMap<Vec<Vec<u16>>, L> = BTreeMap::new();
            for (t, c) in &v {
                for pos in 0..t.len() {
                    let mut e = 0;
                    for (m, w) in t.iter().enumerate() {
                        if m > pos {
                            e += k_exp(m, i as usize, w);
                        }
                    }
                    let mut t2 = t.clone();
                    t2[pos].insert(0, i);
                    let x = r.mul(c, &qpow(r, e));
                    let slot = next.entry(t2).or_insert_with(|| r.zero());
                    *slot = r.add(slot, &x);
                }
            }
            v = next;
        }
    }
    v
}

/// `D/2` for the cyclotomic KLR algebra `R^{λ_m}(ν)`, `ν` the colours of a word:
/// `D = 2(λ_m, ν) − (ν, ν)`.
fn half_shift(s: &Setup, m: usize, word: &[u16]) -> i64 {
    let cartan = s.k.klr.cartan.as_ref().unwrap();
    let lam: i64 = word
        .iter()
        .map(|&c| s.lambdas[m][c as usize] * cartan.d[c as usize])
        .sum();
    let nn: i64 = word
        .iter()
        .flat_map(|&a| {
            word.iter()
                .map(move |&b| cartan.form(a as usize, b as usize))
        })
        .sum();
    assert_eq!((2 * lam - nn) % 2, 0);
    (2 * lam - nn) / 2
}

fn form(
    s: &Setup,
    shs: &mut [Shapovalov<'_>],
    x: &BTreeMap<Vec<Vec<u16>>, L>,
    y: &BTreeMap<Vec<Vec<u16>>, L>,
    r: &PolyRing<Integers>,
) -> L {
    let mut acc = r.zero();
    for (tx, cx) in x {
        for (ty, cy) in y {
            let mut p = r.mul(cx, cy);
            for (m, sh) in shs.iter_mut().enumerate() {
                if r.is_zero(&p) {
                    break;
                }
                let f = sh.form(&tx[m], &ty[m]);
                p = r.mul(&p, &r.mul(&f, &qpow(r, half_shift(s, m, &tx[m]))));
            }
            acc = r.add(&acc, &p);
        }
    }
    acc
}

/// Compares the graded dimensions with the form for every pair of placements; returns the
/// number of nonzero pairs, or an error describing the first mismatch.
fn compare(spec: &str, colours: &[&str]) -> Result<usize, String> {
    let s = setup(spec);
    let r = laurent_ring();
    let cartan = s.k.klr.cartan.clone().unwrap();
    let cols: Vec<u16> = colours
        .iter()
        .map(|c| s.k.klr.colour_index(c).unwrap() as u16)
        .collect();
    let mut shs: Vec<Shapovalov<'_>> = s
        .lambdas
        .iter()
        .map(|l| Shapovalov::new(&cartan, l))
        .collect();
    let pl = placements(&cols, s.lambdas.len() + 1);
    let vecs: Vec<_> = pl.iter().map(|p| vector(&s, p, &r)).collect();
    let loads: Vec<_> = pl.iter().map(|p| loading(&s, p)).collect();
    let mut nonzero = 0;
    for (a, la) in loads.iter().enumerate() {
        for (b, lb) in loads.iter().enumerate() {
            let g = s.k.graded_dimension(la, lb).map_err(|e| e.to_string())?;
            let dim = r.normalize(
                g.iter()
                    .map(|(&d, &n)| (Monomial::from(vec![d as i32]), Int::from(n as i64)))
                    .collect(),
            );
            let f = form(&s, &mut shs, &vecs[a], &vecs[b], &r);
            if dim != f {
                return Err(format!(
                    "{spec}: {pa:?} → {pb:?}: dim {} but form {}",
                    r.fmt_elem(&dim),
                    r.fmt_elem(&f),
                    pa = pl[a],
                    pb = pl[b]
                ));
            }
            if r.is_zero(&dim) {
                continue;
            }
            nonzero += 1;
        }
    }
    Ok(nonzero)
}

#[test]
fn cyclotomic_dimensions_match_tensor_shapovalov() {
    let specs: &[(&str, &[&str])] = &[
        ("klrw type=A1 red=\"i@0, i@10\" quotient=cyclotomic", &["i"]),
        (
            "klrw type=A1 red=\"i@0, i@10\" quotient=cyclotomic",
            &["i", "i"],
        ),
        (
            "klrw type=A1 red=\"2i@0, i@10\" quotient=cyclotomic",
            &["i", "i"],
        ),
        (
            "klrw type=A2 red=\"i@0, j@10\" quotient=cyclotomic",
            &["i", "j"],
        ),
        (
            "klrw type=A1 red=\"2i@0, i@10\" quotient=cyclotomic",
            &["i", "i", "i"],
        ),
        (
            "klrw type=A2 red=\"i+j@0, i@10\" quotient=cyclotomic",
            &["i", "i", "j"],
        ),
    ];
    for (spec, colours) in specs {
        let nonzero = compare(spec, colours).unwrap();
        assert!(nonzero > 0, "{spec}: every pair vanished");
    }
}

/// A letter of an ordered loading, read left to right: a black strand of a colour, or red strand
/// `m` (weight `λ_m`).
#[derive(Clone, Copy, PartialEq)]
enum Letter {
    Black(u16),
    Red(usize),
}

/// The ordered loading of a placement: region `0`, red strand `0`, region `1`, red strand `1`, ….
fn word(regions: &[Vec<u16>]) -> Vec<Letter> {
    let mut w: Vec<Letter> = regions[0].iter().map(|&c| Letter::Black(c)).collect();
    for (m, reg) in regions[1..].iter().enumerate() {
        w.push(Letter::Red(m));
        w.extend(reg.iter().map(|&c| Letter::Black(c)));
    }
    w
}

/// The quantum integer `[n]` at `q^d`, for any integer `n`.
fn qint(r: &PolyRing<Integers>, d: i64, n: i64) -> L {
    let m = n.abs();
    let mut acc = r.zero();
    for k in 0..m {
        acc = r.add(&acc, &qpow(r, d * (m - 1 - 2 * k)));
    }
    if n < 0 { r.neg(&acc) } else { acc }
}

/// `⟨λ − ν, α_i^∨⟩` for the weight at the right end of `w`.
fn prefix_pair(s: &Setup, i: u16, w: &[Letter]) -> i64 {
    let cartan = s.k.klr.cartan.as_ref().unwrap();
    w.iter()
        .map(|l| match *l {
            Letter::Black(j) => -cartan.a[i as usize][j as usize],
            Letter::Red(m) => s.lambdas[m][i as usize],
        })
        .sum()
}

/// The graded dimension of the corner between two ordered loadings, by the deletion recursion
/// of categorification-lean (`Stendhal.cornerPolynomial`, proved equal to the graded dimension
/// of `e_s T^λ e_t` in `Stendhal.sequenceCornerPoly_eq_wordCornerPoly`; B. Webster,
/// arXiv:1309.3796v2, §4.7, Lemma 4.36). A terminal black `i` of one loading is deleted against
/// every black `i` of the other, at prefix `p`, with the factor
/// `q_i^{⟨λ−ν, α_i^∨⟩(u) − 1} [⟨λ−ν, α_i^∨⟩(p)]_{q_i}`; common terminal reds are removed; the
/// empty corner is `1`.
fn corner(s: &Setup, r: &PolyRing<Integers>, a: &[Letter], b: &[Letter]) -> L {
    let (a, b) = match (a.last(), b.last()) {
        (None, None) => return r.one(),
        (Some(Letter::Red(x)), Some(Letter::Red(y))) => {
            return if x == y {
                corner(s, r, &a[..a.len() - 1], &b[..b.len() - 1])
            } else {
                r.zero()
            };
        }
        (Some(Letter::Black(_)), _) => (a, b),
        (_, Some(Letter::Black(_))) => (b, a),
        _ => return r.zero(),
    };
    let Letter::Black(i) = a[a.len() - 1] else {
        unreachable!()
    };
    let u = &a[..a.len() - 1];
    let d = s.k.klr.cartan.as_ref().unwrap().d[i as usize];
    let mut acc = r.zero();
    for (k, l) in b.iter().enumerate() {
        if *l != Letter::Black(i) {
            continue;
        }
        let p = &b[..k];
        let coeff = r.mul(
            &qpow(r, d * (prefix_pair(s, i, u) - 1)),
            &qint(r, d, prefix_pair(s, i, p)),
        );
        let rest: Vec<Letter> = b[..k].iter().chain(&b[k + 1..]).copied().collect();
        acc = r.add(&acc, &r.mul(&coeff, &corner(s, r, u, &rest)));
    }
    acc
}

/// Compares the graded dimensions with the deletion recursion for every pair of placements;
/// returns the number of nonzero pairs.
fn compare_corners(spec: &str, colours: &[&str]) -> Result<usize, String> {
    let s = setup(spec);
    let r = laurent_ring();
    let cols: Vec<u16> = colours
        .iter()
        .map(|c| s.k.klr.colour_index(c).unwrap() as u16)
        .collect();
    let pl = placements(&cols, s.lambdas.len() + 1);
    let loads: Vec<_> = pl.iter().map(|p| loading(&s, p)).collect();
    let words: Vec<_> = pl.iter().map(|p| word(p)).collect();
    let mut nonzero = 0;
    for (a, la) in loads.iter().enumerate() {
        for (b, lb) in loads.iter().enumerate() {
            let g = s.k.graded_dimension(la, lb).map_err(|e| e.to_string())?;
            let dim = r.normalize(
                g.iter()
                    .map(|(&d, &n)| (Monomial::from(vec![d as i32]), Int::from(n as i64)))
                    .collect(),
            );
            let c = corner(&s, &r, &words[a], &words[b]);
            if dim != c {
                return Err(format!(
                    "{spec}: {pa:?} → {pb:?}: dim {} but recursion {}",
                    r.fmt_elem(&dim),
                    r.fmt_elem(&c),
                    pa = pl[a],
                    pb = pl[b]
                ));
            }
            if !r.is_zero(&dim) {
                nonzero += 1;
            }
        }
    }
    Ok(nonzero)
}

#[test]
fn cyclotomic_dimensions_match_lean_corner_recursion() {
    let specs: &[(&str, &[&str])] = &[
        (
            "klrw type=A1 red=\"i@0, i@10\" quotient=cyclotomic",
            &["i", "i"],
        ),
        (
            "klrw type=A2 red=\"i@0, j@10\" quotient=cyclotomic",
            &["i", "j"],
        ),
        (
            "klrw type=A1 red=\"i@0, i@10, i@20\" quotient=cyclotomic",
            &["i", "i"],
        ),
        (
            "klrw type=B2 red=\"i@0, j@10\" quotient=cyclotomic",
            &["i", "j"],
        ),
        (
            "klrw type=B2 red=\"j@0, j@10\" quotient=cyclotomic",
            &["i", "j", "j"],
        ),
        (
            "klrw type=G2 red=\"i@0, j@10\" quotient=cyclotomic",
            &["i", "j"],
        ),
    ];
    for (spec, colours) in specs {
        let nonzero = compare_corners(spec, colours).unwrap();
        assert!(nonzero > 0, "{spec}: every pair vanished");
    }
}

/// Two fundamental `sl_2` factors with one black strand: the Hom matrix of the two boundary
/// projectives (black between the reds, black right of both) is `[[1, q], [q, 1 + q²]]`, as in
/// categorification-lean (`Stendhal.TensorPairing.fundamental_prefix_homGdimPoly_eq_correctedForm`).
/// Webster's printed pairing (arXiv:1309.3796v2, Theorem 4.38) gives `1 + q⁻²` in the corner
/// entry (`Stendhal.TensorPairing.terminal_homGdimPoly_ne_printedForm`).
#[test]
fn two_fundamental_factors_hom_matrix() {
    let s = setup("klrw type=A1 red=\"i@0, i@10\" quotient=cyclotomic");
    let r = laurent_ring();
    let b1 = loading(&s, &[vec![], vec![0], vec![]]);
    let b2 = loading(&s, &[vec![], vec![], vec![0]]);
    let dim = |x: &Loading, y: &Loading| {
        let g = s.k.graded_dimension(x, y).unwrap();
        r.normalize(
            g.iter()
                .map(|(&d, &n)| (Monomial::from(vec![d as i32]), Int::from(n as i64)))
                .collect(),
        )
    };
    assert_eq!(dim(&b1, &b1), r.one());
    assert_eq!(dim(&b1, &b2), qpow(&r, 1));
    assert_eq!(dim(&b2, &b1), qpow(&r, 1));
    assert_eq!(dim(&b2, &b2), r.add(&r.one(), &qpow(&r, 2)));
}
