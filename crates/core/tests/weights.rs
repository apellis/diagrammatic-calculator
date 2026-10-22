//! Region weights in a 2-category: reading `mu=`, the weight of every region, and degrees that
//! depend on the region right of a generator (`h(a)`).

use diagcalc_core::context::{Context, ContextSpec};
use diagcalc_core::pres::Presentation;
use diagcalc_core::ring::Rationals;
use diagcalc_core::term::Letter;

const PRES: &str = r#"
format = "diagcalc-presentation/1"
name = "caps"
title = "Caps with weight-dependent degrees"
colours = "cartan"
objects = "explicit"

[generators.x]
colours = "a"
source = "a"
target = "a"
degree = "(a,a)"
draw = "dot"

[generators.cap]
colours = "a"
source = "a* a"
target = ""
degree = "1 + h(a)"
draw = "cap"
"#;

fn ctx(spec: &str) -> Context<Rationals> {
    let spec = ContextSpec::parse(spec).unwrap();
    Context::new(Presentation::from_toml(PRES).unwrap(), spec, Rationals).unwrap()
}

fn up(c: u16) -> Letter {
    Letter {
        colour: c,
        up: true,
    }
}

fn down(c: u16) -> Letter {
    Letter {
        colour: c,
        up: false,
    }
}

#[test]
fn mu_is_read_and_printed() {
    let c = ctx("caps type=A2 mu=2i-j");
    assert_eq!(c.mu, Some(vec![2, -1]));
    assert_eq!(c.fmt_region(&[2, -1]), "2i-j");
    assert_eq!(c.fmt_region(&[0, 0]), "0");
    assert!(c.spec.print().contains(" mu=2i-j"));
    assert_eq!(ctx("caps type=A1 mu=-3").mu, Some(vec![-3]));
    let spec = ContextSpec::parse("caps type=A2 mu=k").unwrap();
    assert!(Context::new(Presentation::from_toml(PRES).unwrap(), spec, Rationals).is_err());
    let spec = ContextSpec::parse("caps type=A2 mu=3").unwrap();
    assert!(Context::new(Presentation::from_toml(PRES).unwrap(), spec, Rationals).is_err());
}

#[test]
fn regions_follow_from_the_rightmost_one() {
    // in A2, an upward i strand shifts the pairings by (2, -1), a downward j one by (1, -2)
    let c = ctx("caps type=A2 mu=0");
    let obj = [up(0), down(1)];
    assert_eq!(c.region(&obj, 2), Some(vec![0, 0]));
    assert_eq!(c.region(&obj, 1), Some(vec![1, -2]));
    assert_eq!(c.region(&obj, 0), Some(vec![3, -3]));
    assert_eq!(ctx("caps type=A2").region(&obj, 0), None);
}

#[test]
fn degrees_read_the_region_right_of_the_generator() {
    let c = ctx("caps type=A1 mu=1");
    // cap on i* i at the left of i* i: right of it is the region of i* i, whose pairing is 1
    let w = c.parse_term("cap_1 e(i* i i* i)").unwrap();
    let (word, _) = w.terms.first().unwrap();
    assert_eq!(c.word_degree(word), 2);
    // an upward strand to its right raises the pairing by 2
    let w = c.parse_term("cap_1 e(i* i i)").unwrap();
    let (word, _) = w.terms.first().unwrap();
    assert_eq!(c.word_degree(word), 4);
    let w = c.parse_term("cap_2 e(i i* i)").unwrap();
    let (word, _) = w.terms.first().unwrap();
    assert_eq!(c.word_degree(word), 2);
    let c = ctx("caps type=A1 mu=-4");
    let w = c.parse_term("cap_1 e(i* i)").unwrap();
    let (word, _) = w.terms.first().unwrap();
    assert_eq!(c.word_degree(word), -3);
}

#[test]
fn kl_presentation_conforms_at_several_weights() {
    use diagcalc_core::builtin;
    use diagcalc_core::conformance;
    let pres = builtin::presentation("kl").unwrap();
    let reports = conformance::check_presentation(&pres).unwrap();
    assert!(reports.len() > 10);
    for r in &reports {
        let fails: Vec<String> = r
            .failures()
            .map(|f| format!("{}: {}", f.check, f.detail))
            .collect();
        assert!(fails.is_empty(), "{}:\n{}", r.context, fails.join("\n"));
        let typed = r
            .findings
            .iter()
            .filter(|f| f.check.contains("typing"))
            .count();
        eprintln!(
            "{}: {} findings, {typed} typing",
            r.context,
            r.findings.len()
        );
    }
}

fn kl(spec: &str) -> Context<Rationals> {
    let spec = ContextSpec::parse(spec).unwrap();
    let pres = diagcalc_core::builtin::presentation("kl").unwrap();
    Context::new(pres, spec, Rationals).unwrap()
}

#[test]
fn kl_terms_read_and_print() {
    let c = kl("kl type=A1 mu=2");
    // cups name their colour; overloaded x and psi follow the orientation of the strands
    let t = c.parse_term("cap_1 x_2^2 cup_{1,i} e()").unwrap();
    assert_eq!(c.fmt_term(&t), "cap_1 x_2^2 cup_{1,i} e()");
    // degree 2 d_i (m + 1 - n) with m = n = 2
    assert_eq!(c.word_degree(&t.terms[0].0), 2);
    let t = c.parse_term("x_1 psi_1 e(i* i*)").unwrap();
    assert_eq!(c.fmt_term(&t), "x_1 psi_1 e(i* i*)");
    assert!(c.parse_term("cup_1 e()").is_err());
    // sums and exponents that depend on the weight
    let t = c.parse_term("sum_{f=0..h(i)} x_1^{h(i) - f} e(i)").unwrap();
    assert_eq!(c.fmt_term(&t), "e(i) + x_1 e(i) + x_1^2 e(i)");
    assert!(c.parse_term("x_1^{-h(i)} e(i)").is_err());
}

#[test]
fn fake_bubbles_follow_the_grassmannian_relation() {
    // n = <i, mu> = 2: the counterclockwise bubbles of labels -3, -2 are fake, of degree index
    // 0 and 1: 1 and minus the clockwise bubble of degree index 1 (two dots)
    let c = kl("kl type=A1 mu=2");
    let f = |s: &str| c.fmt_term(&c.parse_term(s).unwrap());
    assert_eq!(f("ccw_{i,-3} e()"), "e()");
    assert_eq!(f("ccw_{i,-2} e()"), "-cap_1 x_2^2 cup_{1,i} e()");
    assert_eq!(f("ccw_{i,-4} e()"), "0");
    assert_eq!(f("cw_{i,-1} e()"), "0");
    // index 2: p_1^2 - p_2 with p_j the clockwise bubble of label j + 1
    assert_eq!(
        f("ccw_{i,-1} e()"),
        c.fmt_term(
            &c.parse_term(
                "cap_1 x_2^2 cup_{1,i} cap_1 x_2^2 cup_{1,i} e() - cap_1 x_2^3 cup_{1,i} e()"
            )
            .unwrap()
        )
    );
    // a bubble beside strands sits in the rightmost region; in a tensor factor, in its own
    let t = c.parse_term("ccw_{i,-2} e(i)").unwrap();
    assert_eq!(c.fmt_term(&t), "-cap_2 x_3^2 cup_{2,i} e(i)");
    // left of an upward i strand the pairing is 4, so the label -2 has degree index -1 + 4 + 1
    let t = c.parse_term("ccw_{i,-5} (x) 1_i").unwrap();
    assert_eq!(c.fmt_term(&t), "e(i)");
}
