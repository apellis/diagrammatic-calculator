//! The super Kac-Moody 2-category of Brundan-Ellis: the basis normal form, certified by a Lean
//! theorem for even colours and conditional on the Nondegeneracy Conjecture for odd ones, and the
//! translation into the Khovanov-Lauda presentation for even colours.

use diagcalc_core::context::{Context, ContextSpec};
use diagcalc_core::rep;
use diagcalc_core::ring::Rationals;

fn be2(spec: &str) -> Context<Rationals> {
    let spec = ContextSpec::parse(spec).unwrap();
    let pres = diagcalc_core::builtin::presentation("be2").unwrap();
    Context::new(pres, spec, Rationals).unwrap()
}

fn nf(c: &Context<Rationals>, s: &str) -> String {
    let ev = rep::evaluators(c).unwrap().remove(0);
    let t = c.parse_term(s).unwrap();
    c.fmt_term(&ev.normal_form(&t).unwrap())
}

/// The normal form through a named representation.
fn nf_by(c: &Context<Rationals>, name: &str, s: &str) -> String {
    let decl = c
        .pres
        .representations
        .iter()
        .find(|d| d.name == name)
        .unwrap();
    let ev = rep::evaluator(c, decl).unwrap();
    let t = c.parse_term(s).unwrap();
    c.fmt_term(&ev.normal_form(&t).unwrap())
}

#[test]
fn even_colours_use_the_basis_normal_form_with_a_lean_certificate() {
    let c = be2("be2 type=A2 mu=i");
    assert_eq!(rep::first_applicable(&c).unwrap().name, "skm-basis");
    let ev = rep::evaluators(&c).unwrap().remove(0);
    assert_eq!(ev.provenance().certificate.status, "lean");
    assert!(
        ev.provenance()
            .certificate
            .reference
            .contains("EvenBridge.nondegeneracyConjecture_of_isEven")
    );
    // the translation into kl stays available and gives the same normal forms
    for t in [
        "lcross_1 e(j* i)",
        "cap_3 psi_2 cup_{1,j*} lcross_1 e(j* i)",
        "psi_1 psi_1 e(i* i*)",
    ] {
        assert_eq!(nf(&c, t), nf_by(&c, "kl-translation", t), "{t}");
    }
    // the leftward crossing is the rotation of the upward one
    assert_eq!(nf(&c, "lcross_1 e(j* i)"), "cap_1 psi_2 cup_{3,j} e(j* i)");
    // inversion relations: the leftward crossing is invertible for distinct colours
    assert_eq!(nf(&c, "cap_3 psi_2 cup_{1,j*} lcross_1 e(j* i)"), "e(j* i)");
    // downward crossings square to zero for one colour
    assert_eq!(nf(&c, "psi_1 psi_1 e(i* i*)"), "0");
    // a bubble of negative degree vanishes; of degree zero it is the identity
    let c = be2("be2 type=A1 mu=2");
    assert_eq!(nf(&c, "cap_1 cup_{1,i} e()"), "0");
    assert_eq!(nf(&c, "cap_1 x_1 cup_{1,i} e()"), "e()");
}

#[test]
fn odd_colours_use_the_conditional_basis_normal_form() {
    let c = be2("be2 type=A1 odd=i mu=-1");
    // the translation holds for even colours only
    assert_eq!(rep::first_applicable(&c).unwrap().name, "skm-basis");
    let ev = rep::evaluators(&c).unwrap().remove(0);
    assert_eq!(ev.provenance().certificate.status, "conditional");
    // one odd colour among even ones keeps the certificate conditional
    let c2 = be2("be2 type=B2 odd=j mu=i");
    assert_eq!(rep::first_applicable(&c2).unwrap().name, "skm-basis");
    let ev2 = rep::evaluators(&c2).unwrap().remove(0);
    assert_eq!(ev2.provenance().certificate.status, "conditional");
    assert_eq!(nf(&c, "psi_1 psi_1 e(ii)"), "0");
    // the bubble of index 1 is odd and squares to zero (BE2 (2.18)); the one of index 2 does not
    assert_eq!(nf(&c, "cap_1 x_2 cup_{1,i*} cap_1 x_2 cup_{1,i*} e()"), "0");
    assert_eq!(
        nf(&c, "cap_1 x_2^2 cup_{1,i*} cap_1 x_2^2 cup_{1,i*} e()"),
        "cap_1 x_2^2 cup_{1,i*} cap_1 x_2^2 cup_{1,i*} e()"
    );
}

#[test]
fn signs_with_negative_exponents_are_units() {
    // (-1)^{p(a)(h(a) + 1)} at h(a) = -2 is (-1)^{-1} = -1
    let c = be2("be2 type=A1 odd=i mu=-2");
    let t = c.parse_term("(-1)^{p(i) (h(i) + 1)} e(i)").unwrap();
    assert_eq!(c.fmt_term(&t), "-e(i)");
}
