//! The graded linear-algebra toolkit on cyclotomic quotients.

use diagcalc_core::builtin;
use diagcalc_core::context::{Context, ContextSpec};
use diagcalc_core::quotient::Quotient;
use diagcalc_core::rep;
use diagcalc_core::ring::{Rationals, Ring};
use diagcalc_core::toolkit;

fn ctx<R: Ring>(spec: &str, base: R) -> Context<R> {
    let spec = ContextSpec::parse(spec).unwrap();
    let pres = builtin::presentation(&spec.presentation).unwrap();
    Context::new(pres, spec, base).unwrap()
}

fn quotient_of<'a>(
    c: &'a Context<Rationals>,
    ev: &'a dyn rep::Evaluator<Rationals>,
) -> Quotient<'a, Rationals> {
    Quotient::from_context(c, ev).unwrap()
}

#[test]
fn radical_blocks_and_centre() {
    // (spec, dimension, radical, centre dimensions in degrees 0, 2, 4)
    for (spec, dim, rad, centre) in [
        // NH_2 at level 2 is a matrix algebra of size 2
        ("klr type=A1 nu=2i Lambda=2i", 4, 0, [1, 0, 0]),
        // V(2Λ_1) of sl_3, weight space of multiplicity 1: one simple module of dimension 3
        ("klr type=A2 nu=2i+j Lambda=2i", 18, 9, [1, 1, 0]),
        // the adjoint representation, a root weight: semisimple
        ("klr type=A2 nu=2i+j Lambda=i+j", 9, 0, [1, 0, 0]),
        // the zero weight of the adjoint representation (multiplicity 2): two one-dimensional
        // simple modules in one block; the centre has the Betti numbers 1, 2 of the minimal
        // resolution of C^2/Z_3
        ("klr type=A2 nu=i+j Lambda=i+j", 6, 4, [1, 2, 0]),
    ] {
        let c = ctx(spec, Rationals);
        let evs = rep::evaluators(&c).unwrap();
        let q = quotient_of(&c, evs[0].as_ref());
        assert_eq!(toolkit::basis(&q).unwrap().len(), dim, "{spec}");
        assert_eq!(toolkit::radical(&q).unwrap().len(), rad, "{spec}");
        assert_eq!(toolkit::semisimple_dimension(&q).unwrap(), dim - rad);
        let z: Vec<usize> = [0, 2, 4]
            .iter()
            .map(|&d| toolkit::centre(&q, d).unwrap().len())
            .collect();
        assert_eq!(z, centre, "{spec}");
        // cyclotomic KLR algebras of one weight are indecomposable
        let b = toolkit::blocks(&q).unwrap();
        assert_eq!(b.len(), 1, "{spec}");
    }
}

#[test]
fn idempotents() {
    let c = ctx("klr type=A2 nu=2i+j Lambda=2i", Rationals);
    let evs = rep::evaluators(&c).unwrap();
    let q = quotient_of(&c, evs[0].as_ref());
    let t = |s: &str| c.parse_term(s).unwrap();
    // e(iji) R e(iji) has dimensions 1, 1 in degrees 0, 2: local
    assert!(toolkit::is_primitive(&q, &t("e(iji)")).unwrap());
    // e(iij) splits through the nilHecke algebra on its first two strands
    assert!(!toolkit::is_primitive(&q, &t("e(iij)")).unwrap());
    let mut found = 0;
    for s in [
        "x_1 psi_1 e(iij)",
        "psi_1 x_2 e(iij)",
        "-psi_1 x_1 e(iij)",
        "-x_2 psi_1 e(iij)",
    ] {
        let e = t(s);
        if toolkit::mul(&q, &e, &e).unwrap() == q.reduce(&e).unwrap() {
            assert!(toolkit::is_primitive(&q, &e).unwrap(), "{s}");
            found += 1;
        }
    }
    assert_eq!(found, 2);
    // lifting: an idempotent plus a radical element of positive degree lifts to an idempotent
    let rad = toolkit::radical(&q).unwrap();
    let r = rad
        .iter()
        .find(|r| {
            let (w, _) = &r.terms[0];
            w.source == c.objects[1] && c.target(w) == c.objects[1]
        })
        .cloned();
    if let Some(r) = r {
        let e = c.add(&t("e(iji)"), &r);
        let lifted = toolkit::lift_idempotent(&q, &e, 32).unwrap();
        assert_eq!(toolkit::mul(&q, &lifted, &lifted).unwrap(), lifted);
        assert!(toolkit::is_primitive(&q, &lifted).unwrap());
    }
    // a truncation has no radical
    let c = ctx("klr type=A2 nu=2i+j degcap=2", Rationals);
    let evs = rep::evaluators(&c).unwrap();
    let q = quotient_of(&c, evs[0].as_ref());
    assert!(toolkit::radical(&q).is_err());
}
