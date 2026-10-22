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
