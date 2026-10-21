//! End-to-end checks of elaboration, the equality strategies and the conformance checker.

use diagcalc_core::builtin;
use diagcalc_core::conformance::{self, Status};
use diagcalc_core::context::{Context, ContextSpec};
use diagcalc_core::poly::{Monomial, Poly};
use diagcalc_core::pres::Presentation;
use diagcalc_core::rep::{self, Evaluator};
use diagcalc_core::ring::{Integers, Rationals, Ring};

fn ctx<R: Ring>(spec: &str, base: R) -> Context<R> {
    let spec = ContextSpec::parse(spec).unwrap();
    let pres = builtin::presentation(&spec.presentation).unwrap();
    Context::new(pres, spec, base).unwrap()
}

fn nf<R: Ring>(c: &Context<R>, ev: &dyn Evaluator<R>, s: &str) -> String {
    c.fmt_term(&ev.normal_form(&c.parse_term(s).unwrap()).unwrap())
}

#[test]
fn checker_counts_instances() {
    let reports = conformance::check_builtin().unwrap();
    let a2 = reports
        .iter()
        .find(|r| r.context.contains("type=A2"))
        .unwrap();
    let held = a2
        .findings
        .iter()
        .filter(|f| f.check.contains("holds in") && f.status == Status::Pass)
        .count();
    // 2 + 2 dot slides (same: 2 colours, distinct: 2 ordered pairs), squares 2 + 2,
    // corrected braids 2, plain braids 8 − 2 = 6
    assert_eq!(held, 4 + 4 + 4 + 2 + 6);
}

fn with_edit(name: &str, from: &str, to: &str) -> Presentation {
    let (_, text) = builtin::PRESENTATIONS
        .iter()
        .find(|(n, _)| *n == name)
        .unwrap();
    assert!(text.contains(from));
    Presentation::from_toml(&text.replace(from, to)).unwrap()
}

#[test]
fn checker_catches_wrong_relations() {
    let bad = with_edit("klr", "e(a b a) + Qbar_ab", "e(a b a) - Qbar_ab");
    let reports = conformance::check_presentation(&bad).unwrap();
    assert!(
        reports
            .iter()
            .any(|r| r.failures().any(|f| f.check.contains("braid-corrected")))
    );

    let bad = with_edit(
        "odd-nilhecke",
        "rhs = \"-psi_1 x_2 e(i i) + e(i i)\"",
        "rhs = \"psi_1 x_2 e(i i) + e(i i)\"",
    );
    let reports = conformance::check_presentation(&bad).unwrap();
    assert!(reports.iter().any(|r| !r.ok()));

    let bad = with_edit("klr", "degree = \"-(a,b)\"", "degree = \"(a,b)\"");
    let reports = conformance::check_presentation(&bad).unwrap();
    assert!(
        reports
            .iter()
            .any(|r| r.failures().any(|f| f.check.ends_with("typing")))
    );
}

#[test]
fn nilhecke_strategies_agree() {
    let c = ctx("nilhecke nu=3", Rationals);
    let evs = rep::evaluators(&c).unwrap();
    assert_eq!(evs.len(), 2);
    let samples = [
        "psi_1 x_1",
        "x_1 psi_1",
        "psi_1 psi_2 psi_1 x_1^2 x_2",
        "psi_2 x_1 psi_1 x_3 psi_2 + 3 x_2^2",
        "(psi_1 + x_2)^3",
        "psi_1 psi_2 x_1^3 x_2 psi_1 psi_2",
    ];
    for s in samples {
        let t = c.parse_term(s).unwrap();
        let a = evs[0].normal_form(&t).unwrap();
        let b = evs[1].normal_form(&t).unwrap();
        // each normal form equals the term in the other strategy
        assert!(evs[0].is_zero(&c.sub(&t, &b)).unwrap(), "{s}");
        assert!(evs[1].is_zero(&c.sub(&t, &a)).unwrap(), "{s}");
    }
    assert_eq!(nf(&c, evs[1].as_ref(), "psi_1 x_1"), "1 + x_2 psi_1");
    assert_eq!(nf(&c, evs[0].as_ref(), "x_1 psi_1"), "1 + psi_1 x_2");
    assert_eq!(nf(&c, evs[0].as_ref(), "psi_1 x_1"), "psi_1 x_1");
}

/// All monomials in the first `n` variables of total degree at most `d`.
fn monomials<R: Ring>(
    ring: &diagcalc_core::poly::PolyRing<R>,
    n: usize,
    d: i32,
) -> Vec<Poly<R::Elem>> {
    let mut out = vec![];
    let nv = ring.nvars();
    let mut e = vec![0i32; n];
    loop {
        if e.iter().sum::<i32>() <= d {
            let mut m = vec![0; nv];
            m[..n].copy_from_slice(&e);
            out.push(ring.monomial(Monomial::from(m), ring.base.one()));
        }
        let mut k = 0;
        loop {
            if k == n {
                return out;
            }
            e[k] += 1;
            if e[k] <= d {
                break;
            }
            e[k] = 0;
            k += 1;
        }
    }
}

/// The normal form acts like the term, and every relation acts by zero.
fn check_actions<R: Ring>(c: &Context<R>, samples: &[&str], deg: i32) {
    for ev in rep::evaluators(c).unwrap() {
        for s in samples {
            let t = c.parse_term(s).unwrap();
            let n = ev.normal_form(&t).unwrap();
            for obj in &c.objects {
                let ring = ev.poly_ring(obj.len());
                for f in monomials(&ring, obj.len(), deg) {
                    assert_eq!(
                        ev.act(&t, obj, &f).unwrap(),
                        ev.act(&n, obj, &f).unwrap(),
                        "{s}"
                    );
                }
            }
        }
        for rel in &c.pres.relations {
            for b in conformance::instances(c, rel).unwrap() {
                let (l, r) = conformance::instance_terms(c, rel, &b).unwrap();
                let d = c.sub(&l, &r);
                let objs: std::collections::BTreeSet<_> =
                    d.terms.iter().map(|(w, _)| w.source.clone()).collect();
                for obj in objs {
                    let ring = ev.poly_ring(obj.len());
                    for f in monomials(&ring, obj.len(), deg) {
                        assert!(
                            ev.act(&d, &obj, &f).unwrap().is_empty(),
                            "{} {:?}",
                            rel.name,
                            b
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn actions_match_normal_forms() {
    check_actions(
        &ctx("nilhecke nu=3", Integers),
        &["psi_1 psi_2 x_1^2 psi_1", "(psi_2 + x_3)^3"],
        3,
    );
    check_actions(
        &ctx("odd-nilhecke nu=3", Integers),
        &[
            "psi_1 psi_2 x_1^2 psi_1",
            "(psi_2 + x_3)^3",
            "psi_1 x_3 psi_2",
        ],
        3,
    );
    check_actions(
        &ctx("klr type=B2 nu=2i+j", Integers),
        &["psi_1 psi_2 e(iji)", "psi_2 psi_1 x_3 psi_2 e(iij)"],
        2,
    );
}

#[test]
fn odd_divided_differences() {
    let c = ctx("odd-nilhecke nu=2", Integers);
    let ev = rep::evaluators(&c).unwrap().pop().unwrap();
    let ring = ev.poly_ring(2);
    let psi = c.parse_term("psi_1").unwrap();
    let obj = &c.objects[0];
    let x = |a: i32, b: i32| ring.monomial(Monomial::from(vec![a, b]), ring.base.one());
    let act = |f: Poly<_>| {
        ev.act(&psi, obj, &f)
            .unwrap()
            .pop()
            .map(|p| p.1)
            .unwrap_or(ring.zero())
    };
    assert_eq!(act(x(2, 0)), ring.sub(&x(1, 0), &x(0, 1)));
    assert_eq!(act(x(2, 1)), x(1, 1));
    assert_eq!(act(x(1, 2)), ring.neg(&x(1, 1)));
    assert_eq!(act(x(1, 0)), ring.one());
    assert_eq!(act(x(0, 1)), ring.one());
    // distant crossings anticommute
    let c4 = ctx("odd-nilhecke nu=4", Integers);
    let ev4 = rep::evaluators(&c4).unwrap().pop().unwrap();
    assert!(
        ev4.is_zero(&c4.parse_term("psi_1 psi_3 + psi_3 psi_1").unwrap())
            .unwrap()
    );
    assert!(!ev4.is_zero(&c4.parse_term("psi_1 psi_3").unwrap()).unwrap());
}

#[test]
fn klr_iji_idempotents() {
    let c = ctx("klr type=A2 nu=2i+j", Integers);
    let ev = rep::evaluators(&c).unwrap().pop().unwrap();
    let left = c.parse_term("psi_1 psi_2 psi_1 e(iji)").unwrap();
    let right = c.parse_term("-psi_2 psi_1 psi_2 e(iji)").unwrap();
    let e = c.parse_term("e(iji)").unwrap();
    assert!(ev.is_zero(&c.sub(&c.add(&left, &right), &e)).unwrap());
    assert!(ev.is_zero(&c.sub(&c.compose(&left, &left), &left)).unwrap());
    assert!(ev.is_zero(&c.compose(&left, &right)).unwrap());
    assert!(!ev.is_zero(&left).unwrap());
}

#[test]
fn klr_normal_forms() {
    let c = ctx("klr type=A2 nu=i+j", Integers);
    let ev = rep::evaluators(&c).unwrap().pop().unwrap();
    assert_eq!(
        nf(&c, ev.as_ref(), "psi_1 psi_1 e(ij)"),
        "x_1 e(ij) + x_2 e(ij)"
    );
    assert_eq!(nf(&c, ev.as_ref(), "x_2 psi_1 e(ij)"), "psi_1 x_1 e(ij)");
    let c = ctx("klr type=A2 nu=2i", Integers);
    let ev = rep::evaluators(&c).unwrap().pop().unwrap();
    assert_eq!(nf(&c, ev.as_ref(), "x_1 psi_1"), "1 + psi_1 x_2");
}

#[test]
fn parameters_and_scalars() {
    // a non-default Q with a scalar
    let c = ctx("klr type=A2 nu=i+j scalars=t Q_ij=\"t u + v\"", Rationals);
    let ev = rep::evaluators(&c).unwrap().pop().unwrap();
    assert_eq!(
        nf(&c, ev.as_ref(), "psi_1 psi_1 e(ji)"),
        "x_1 e(ji) + t x_2 e(ji)"
    );
    let reports = conformance::check(&c);
    assert!(reports.ok(), "{:?}", reports.failures().collect::<Vec<_>>());
}

#[test]
fn shipped_presentations_are_canonical() {
    for (name, text) in builtin::PRESENTATIONS {
        let p = Presentation::from_toml(text).unwrap();
        let printed = p.to_toml();
        assert_eq!(Presentation::from_toml(&printed).unwrap(), p, "{name}");
        assert_eq!(printed, text, "{name} is not in canonical form");
    }
}

#[test]
fn context_lines_round_trip() {
    for s in [
        "klr type=A2 nu=2i+j ring=QQ",
        "klr type=G2 colours=a,b odd=a nu=a+b ring=ZZ scalars=t Q_ab=\"u + t v^3\"",
        "nilhecke nu=4 ring=GF(5)",
    ] {
        let spec = ContextSpec::parse(s).unwrap();
        assert_eq!(ContextSpec::parse(&spec.print()).unwrap(), spec);
        assert_eq!(spec.print(), s);
    }
}

#[test]
fn printed_terms_parse_back() {
    let cases = [
        (
            "odd-nilhecke nu=3 ring=ZZ",
            "(psi_1 + x_2)^3 + x_3 x_1 - psi_2 x_1 psi_1 x_3",
        ),
        ("nilhecke nu=3 ring=QQ", "1/2 (psi_1 + x_2)^3 psi_2"),
        (
            "klr type=G2 nu=i+2j ring=ZZ",
            "psi_1 psi_2 psi_1 e(jij) + x_3^2 e(ijj)",
        ),
        (
            "klr type=A2 nu=i+j ring=ZZ[I]",
            "(1+I) psi_1 e(ij) - I x_2 e(ji)",
        ),
    ];
    for (spec, src) in cases {
        let check = |text: &str| {
            let spec = ContextSpec::parse(spec).unwrap();
            let r = diagcalc_core::ops::simplify(
                &diagcalc_core::ops::Request {
                    context: spec.print(),
                    ..Default::default()
                },
                text,
            );
            assert_eq!(r["status"], "ok", "{r}");
            r["normal_form"].as_str().unwrap().to_string()
        };
        let nf = check(src);
        assert_eq!(check(&nf), nf, "{spec}: {src}");
    }
}

#[test]
fn term_text_round_trips() {
    let c = ctx("odd-nilhecke nu=4", Integers);
    for src in [
        "x_3 x_1 psi_2 x_4 - 2 psi_1 psi_3 x_2",
        "(x_1 + psi_3)^2 x_2 x_4 psi_1",
        "psi_3 psi_1 psi_2 psi_3 psi_1",
    ] {
        let t = c.parse_term(src).unwrap();
        let printed = c.fmt_term(&t);
        assert_eq!(
            c.parse_term(&printed).unwrap(),
            t,
            "{src} printed as {printed}"
        );
    }
}
