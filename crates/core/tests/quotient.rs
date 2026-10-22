//! Cyclotomic quotients and truncations: graded dimensions against the q-Shapovalov form.

use diagcalc_core::builtin;
use diagcalc_core::context::{Context, ContextSpec};
use diagcalc_core::quotient::Quotient;
use diagcalc_core::rep;
use diagcalc_core::ring::{Rationals, Ring};
use diagcalc_core::shapovalov::Shapovalov;
use std::collections::BTreeMap;

fn ctx<R: Ring>(spec: &str, base: R) -> Context<R> {
    let spec = ContextSpec::parse(spec).unwrap();
    let pres = builtin::presentation(&spec.presentation).unwrap();
    Context::new(pres, spec, base).unwrap()
}

/// `dim_q e(t) R^Λ(ν) e(s) = q^{D/2} ⟨F_{t'} v_Λ, F_{s'} v_Λ⟩` with `D = 2(Λ, ν) − (ν, ν)` and
/// `s'`, `t'` the reversed sequences (the leftmost strand is the first `F` applied), for every
/// pair of objects, in every computed degree.
fn check_against_shapovalov(spec: &str) -> usize {
    let c = ctx(spec, Rationals);
    let evs = rep::evaluators(&c).unwrap();
    let cartan = c.cartan.clone().unwrap();
    let lam: Vec<i64> = c
        .parse_weight(c.spec.lambda.as_ref().unwrap())
        .unwrap()
        .iter()
        .map(|&x| x as i64)
        .collect();
    let nu = c.weight.clone().unwrap();
    let n = nu.len();
    let d_top: i64 = 2
        * (0..n)
            .map(|i| lam[i] * cartan.d[i] * nu[i] as i64)
            .sum::<i64>()
        - (0..n)
            .flat_map(|i| (0..n).map(move |j| (i, j)))
            .map(|(i, j)| nu[i] as i64 * nu[j] as i64 * cartan.form(i, j))
            .sum::<i64>();
    assert_eq!(d_top % 2, 0);
    let mut sh = Shapovalov::new(&cartan, &lam);
    let mut total = 0;
    for ev in &evs {
        let q = Quotient::from_context(&c, ev.as_ref()).unwrap();
        for s in &c.objects {
            let s_rev: Vec<u16> = s.iter().rev().map(|l| l.colour).collect();
            for t in &c.objects {
                let t_rev: Vec<u16> = t.iter().rev().map(|l| l.colour).collect();
                let form = sh.form(&t_rev, &s_rev);
                let mut want: BTreeMap<i64, usize> = BTreeMap::new();
                for (m, k) in form.terms.iter() {
                    let k: i64 = k.to_string().parse().unwrap();
                    assert!(k > 0, "{spec}: negative coefficient in the form");
                    want.insert(m.degree() + d_top / 2, k as usize);
                }
                let got = q.graded_dimension(s, Some(t)).unwrap();
                assert_eq!(
                    got,
                    want,
                    "{spec}: e({}) R e({})",
                    c.fmt_object(t),
                    c.fmt_object(s)
                );
                total += got.values().sum::<usize>();
            }
        }
    }
    total
}

#[test]
fn cyclotomic_type_a() {
    assert_eq!(check_against_shapovalov("klr type=A2 nu=i+j Lambda=i"), 1);
    check_against_shapovalov("klr type=A2 nu=2i+j Lambda=2i");
    check_against_shapovalov("klr type=A2 nu=2i+j Lambda=i+j");
    check_against_shapovalov("klr type=A2 nu=2i+2j Lambda=2i+j");
    check_against_shapovalov("klr type=A3 nu=i+j+k Lambda=i");
    check_against_shapovalov("klr type=A3 nu=i+2j+k Lambda=j+k");
}

#[test]
fn cyclotomic_types_b_g() {
    check_against_shapovalov("klr type=B2 nu=i+j Lambda=i+j");
    check_against_shapovalov("klr type=B2 nu=2i+j Lambda=2i");
    check_against_shapovalov("klr type=B2 nu=i+2j Lambda=j");
    check_against_shapovalov("klr type=B2 nu=i+2j Lambda=i+j");
    check_against_shapovalov("klr type=G2 nu=i+j Lambda=i+j");
    check_against_shapovalov("klr type=G2 nu=2i+j Lambda=i");
    check_against_shapovalov("klr type=G2 nu=i+2j Lambda=j");
}

#[test]
fn nilhecke_truncation_and_dot_cap() {
    let c = ctx("nilhecke nu=2 Lambda=2 degcap=6", Rationals);
    let evs = rep::evaluators(&c).unwrap();
    for ev in &evs {
        let q = Quotient::from_context(&c, ev.as_ref()).unwrap();
        let g = q.graded_dimension(&c.objects[0], None).unwrap();
        assert_eq!(g, BTreeMap::from([(-2, 1), (0, 2), (2, 1)]));
    }
    // without a Cartan datum, a cyclotomic quotient needs a degree cap
    let c = ctx("nilhecke nu=2 Lambda=2", Rationals);
    let evs = rep::evaluators(&c).unwrap();
    assert!(Quotient::from_context(&c, evs[0].as_ref()).is_err());
    // x_k^2 = 0 on every strand: NH_2 / (x_1^2, x_2^2) has the words with exponents below 2
    let c = ctx("nilhecke nu=2 dotcap=2", Rationals);
    let evs = rep::evaluators(&c).unwrap();
    let q = Quotient::from_context(&c, evs[0].as_ref()).unwrap();
    let g = q.graded_dimension(&c.objects[0], None).unwrap();
    eprintln!("{g:?}");
}

#[test]
#[ignore]
fn timings() {
    for spec in [
        "klr type=A2 nu=2i+j Lambda=2i",
        "klr type=A2 nu=2i+2j Lambda=2i+j",
        "klr type=A3 nu=i+j+k Lambda=i",
        "klr type=A3 nu=i+2j+k Lambda=j+k",
        "klr type=G2 nu=2i+j Lambda=i",
    ] {
        let t = std::time::Instant::now();
        let n = check_against_shapovalov(spec);
        eprintln!("{spec}: dim {n}, {:?}", t.elapsed());
    }
}
