//! A vanishing bound is not a computational cutoff: high-degree inputs still have exact values.

use diagcalc_core::builtin;
use diagcalc_core::context::{Context, ContextSpec};
use diagcalc_core::ops::{self, Request};
use diagcalc_core::quotient::Quotient;
use diagcalc_core::rep::{self, Evaluator};
use diagcalc_core::ring::Rationals;
use diagcalc_core::toolkit;

fn request(context: &str) -> Request {
    Request {
        context: context.into(),
        presentation: None,
        strategy: None,
    }
}

fn context(text: &str) -> Context<Rationals> {
    let spec = ContextSpec::parse(text).unwrap();
    let pres = builtin::presentation(&spec.presentation).unwrap();
    Context::new(pres, spec, Rationals).unwrap()
}

#[test]
fn proven_vanishing_accepts_arbitrarily_high_input_degrees() {
    // In the one-strand cyclotomic algebra Q[x]/(x^11), x^10 survives and x^11 vanishes.
    let req = request("klr type=A1 nu=i Lambda=11i ring=QQ");
    for exponent in [11, 20, 50] {
        let v = ops::simplify(&req, &format!("x_1^{exponent} e(i) + x_1^10 e(i)"));
        assert_eq!(v["status"], "ok", "{v}");
        assert_eq!(v["normal_form"], "x_1^10", "{v}");
        assert!(!v["provenance"]["notes"].to_string().contains("truncated"));
        let v = ops::check(&req, "zero", &[format!("x_1^{exponent} e(i)")]);
        assert_eq!(v["holds"], true, "{v}");
    }
}

#[test]
fn basis_queries_above_a_proven_bound_are_empty() {
    let c = context("klr type=A1 nu=i Lambda=11i ring=QQ");
    let q = Quotient::from_context(&c, rep::evaluators(&c).unwrap().remove(0)).unwrap();
    assert!(q.complete());
    assert_eq!(q.graded_basis(&c.objects[0], 20).unwrap().len(), 1);
    assert!(q.piece(&c.objects[0], 40).unwrap().basis.is_empty());
    assert!(q.graded_basis(&c.objects[0], 40).unwrap().is_empty());
    assert!(
        toolkit::piece(&q, Some(&c.objects[0]), None, 40)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn dot_cap_vanishing_is_exact_too() {
    let req = request("nilhecke nu=1 dotcap=3 ring=QQ");
    let v = ops::simplify(&req, "x_1^20 + x_1^2");
    assert_eq!(v["status"], "ok", "{v}");
    assert_eq!(v["normal_form"], "x_1^2", "{v}");
}

#[test]
fn an_explicit_truncation_never_silently_drops_unknown_components() {
    for ctx in [
        "klr type=A1 nu=i ring=QQ degcap=6",
        "klr type=A1 nu=i Lambda=11i ring=QQ degcap=6",
    ] {
        let v = ops::simplify(&request(ctx), "x_1^4 e(i)");
        assert_eq!(v["status"], "error", "{ctx}: {v}");
        assert_eq!(v["kind"], "precondition", "{v}");
        assert!(v["message"].as_str().unwrap().contains("8"), "{v}");
    }
    // A separately proved vanishing statement remains valid even with a smaller requested cap.
    let v = ops::simplify(
        &request("klr type=A1 nu=i Lambda=11i ring=QQ degcap=6"),
        "x_1^20 e(i) + e(i)",
    );
    assert_eq!(v["status"], "ok", "{v}");
    assert_eq!(v["normal_form"], "1", "{v}");
}

#[test]
fn edited_cyclotomic_idempotent_example_runs_watch_and_solve() {
    let src = "diagcalc-workspace/1
context klr type=A2 nu=2i+j ring=QQ Lambda=2i+j
def e := c1 x_2 x_3 e(iji) + c2 psi_1 x_2 psi_2 psi_1 e(iji)
watch e e - e
solve e e - e = 0 for c1, c2
";
    let v = ops::run_workspace(src, None);
    assert_eq!(v["status"], "ok", "{v}");
    for cell in v["cells"].as_array().unwrap() {
        assert_eq!(cell["status"], "ok", "{cell}");
    }
    assert_ne!(v["cells"][2]["outcome"], "limit_reached", "{v}");
}

#[test]
fn a_quotient_without_a_known_global_bound_computes_requested_degrees() {
    for name in ["nilhecke", "odd-nilhecke"] {
        let spec = format!("{name} nu=1 Lambda=11 ring=QQ");
        let v = ops::simplify(&request(&spec), "x_1^20 + x_1^10");
        assert_eq!(v["status"], "ok", "{name}: {v}");
        assert_eq!(v["normal_form"], "x_1^10", "{v}");
        assert!(!v["provenance"]["notes"].to_string().contains("truncated"));
        let c = context(&spec);
        let q = Quotient::from_context(&c, rep::evaluators(&c).unwrap().remove(0)).unwrap();
        // No global bound was established: do not pretend to enumerate the whole algebra.
        assert!(!q.complete());
        assert!(!q.truncated);
        assert_eq!(q.graded_basis(&c.objects[0], 20).unwrap().len(), 1);
        assert!(q.graded_basis(&c.objects[0], 40).unwrap().is_empty());
        assert!(q.basis().is_err());
        assert!(q.graded_dimension(&c.objects[0], None).is_err());
        assert!(toolkit::basis(&q).is_err());
    }
}

#[test]
fn truncated_basis_queries_still_recognize_proven_vanishing() {
    let c = context("klr type=A1 nu=i Lambda=11i ring=QQ degcap=6");
    let q = Quotient::from_context(&c, rep::evaluators(&c).unwrap().remove(0)).unwrap();
    assert!(q.truncated);
    assert!(toolkit::piece(&q, Some(&c.objects[0]), None, 8).is_err());
    assert!(
        toolkit::piece(&q, Some(&c.objects[0]), None, 40)
            .unwrap()
            .is_empty()
    );
    let alg = toolkit::Ambient { ctx: &c, eval: &q };
    assert!(
        toolkit::piece(&alg, Some(&c.objects[0]), None, 40)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn uncapped_high_degree_arithmetic_remains_nonzero_and_respects_relations() {
    let klr = request("klr type=A2 nu=2i+j ring=QQ");
    for degree in [16, 20, 40, 100] {
        let power = (degree + 2) / 2;
        let lhs = format!("x_1^{power} psi_1 e(iij)");
        // Iterating the defining dot-slide relation gives this divided-difference formula.
        let mut rhs = format!("psi_1 x_2^{power} e(iij)");
        for k in 0..power {
            rhs.push_str(&format!(" + x_1^{k} x_2^{} e(iij)", power - 1 - k));
        }
        let v = ops::check(&klr, "equal", &[lhs.clone(), rhs]);
        assert_eq!(v["holds"], true, "degree {degree}: {v}");
        let v = ops::check(&klr, "zero", &[lhs]);
        assert_eq!(v["holds"], false, "degree {degree}: {v}");
        let power = degree / 2 - 1;
        let odd = request("odd-nilhecke nu=2 ring=QQ");
        let v = ops::check(
            &odd,
            "zero",
            &[format!("x_1^{power} x_2 + x_2 x_1^{power}")],
        );
        assert_eq!(v["holds"], true, "degree {degree}: {v}");
        let v = ops::check(&odd, "zero", &[format!("x_1^{power} x_2")]);
        assert_eq!(v["holds"], false, "degree {degree}: {v}");
    }
}

#[test]
fn two_category_bubbles_at_degrees_sixteen_and_twenty_need_no_cutoff() {
    for spec in [
        "kl type=A1 mu=-1 ring=QQ",
        "be2 type=A1 odd=i mu=-1 ring=QQ",
    ] {
        let c = context(spec);
        let req = request(spec);
        for degree in [16, 20] {
            let term = format!("cap_1 x_2^{} cup_{{1,i*}} e()", degree / 2);
            let t = c.parse_term(&term).unwrap();
            assert_eq!(c.word_degree(&t.terms[0].0), degree);
            let v = ops::simplify(&req, &term);
            assert_eq!(v["status"], "ok", "{spec}: {v}");
            assert_eq!(v["normal_form"], term, "{v}");
            if spec.starts_with("be2") {
                assert_eq!(v["provenance"]["certificate"]["status"], "conditional");
            }
        }
    }
}
