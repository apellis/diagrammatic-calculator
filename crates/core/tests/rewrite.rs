//! Rewriting: matching up to interchange, application with provenance, and straightening by
//! rewriting checked against the representations.

use diagcalc_core::builtin;
use diagcalc_core::context::{Context, ContextSpec};
use diagcalc_core::rep;
use diagcalc_core::rewrite::{self, Rect};
use diagcalc_core::ring::{Integers, Ring};

fn ctx<R: Ring>(spec: &str, base: R) -> Context<R> {
    let spec = ContextSpec::parse(spec).unwrap();
    let pres = builtin::presentation(&spec.presentation).unwrap();
    Context::new(pres, spec, base).unwrap()
}

/// A deterministic pseudo-random word of `len` generators on the context's objects, as text.
fn random_words<R: Ring>(c: &Context<R>, count: usize, len: usize, seed: u64) -> Vec<String> {
    let mut x = seed;
    let mut next = |m: usize| {
        x = x
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((x >> 33) as usize) % m
    };
    let n = c.objects[0].len();
    let mut out = Vec::new();
    for _ in 0..count {
        let obj = &c.objects[next(c.objects.len())];
        let mut parts = Vec::new();
        for _ in 0..len {
            if n > 1 && next(3) > 0 {
                parts.push(format!("psi_{}", next(n - 1) + 1));
            } else {
                parts.push(format!("x_{}", next(n) + 1));
            }
        }
        parts.push(format!("e({})", c.fmt_object(obj)));
        out.push(parts.join(" "));
    }
    out
}

/// Straightening by rewriting reaches the normal form of each representation whose basis it
/// targets, and every intermediate term has the same value.
fn check_straighten<R: Ring>(spec: &str, base: R, count: usize, len: usize) {
    let c = ctx(spec, base);
    let rules = rewrite::rules(&c).unwrap();
    for (decl, ev) in c
        .pres
        .representations
        .iter()
        .zip(rep::evaluators(&c).unwrap())
    {
        let layout = rep::basis_layout(&c, decl).unwrap();
        let mut steps = 0;
        for s in random_words(&c, count, len, 7) {
            let t = c.parse_term(&s).unwrap();
            let chain = rewrite::straighten(&c, &rules, &layout, &t, 100_000).unwrap();
            let want = ev.normal_form(&t).unwrap();
            steps += chain.steps.len();
            assert_eq!(
                c.fmt_term(&chain.end),
                c.fmt_term(&want),
                "{spec} {}: {s}",
                decl.name
            );
            for (k, tk) in chain.terms(&c).iter().enumerate().step_by(7) {
                assert_eq!(
                    ev.normal_form(tk).unwrap(),
                    want,
                    "{spec}: {s}, after step {k}"
                );
            }
        }
        eprintln!("{spec} {}: {steps} steps", decl.name);
        assert!(steps > count, "{spec}: too few steps to test anything");
    }
}

#[test]
fn straighten_nilhecke() {
    check_straighten("nilhecke nu=3", Integers, 30, 6);
    check_straighten("nilhecke nu=4", Integers, 10, 8);
}

#[test]
fn straighten_odd_nilhecke() {
    check_straighten("odd-nilhecke nu=3", Integers, 30, 6);
    check_straighten("odd-nilhecke nu=4", Integers, 10, 8);
}

#[test]
fn straighten_klr() {
    check_straighten("klr type=A2 nu=i+j", Integers, 20, 5);
    check_straighten("klr type=A2 nu=2i+j", Integers, 30, 6);
    check_straighten("klr type=B2 nu=2i+j", Integers, 30, 6);
    check_straighten("klr type=G2 nu=i+2j", Integers, 20, 6);
    check_straighten("klr type=A3 nu=i+j+k+i", Integers, 10, 8);
}

#[test]
fn matching_up_to_interchange() {
    let c = ctx("klr type=A2 nu=i+j", Integers);
    let rules = rewrite::rules(&c).unwrap();
    // A dot above the crossing on its right output, written with the dot on the left strand
    // first: `x_2 psi_1 e(ij)` is the backward pattern of dot-slide-right-distinct.
    let t = c.parse_term("psi_1 x_2 psi_1 e(ij)").unwrap();
    let w = &t.terms[0].0;
    let ms = rewrite::find_matches(&c, &rules, w, None);
    let names: Vec<(String, bool)> = ms
        .iter()
        .map(|m| (rules[m.rule].relation.clone(), rules[m.rule].forward))
        .collect();
    assert!(names.contains(&("dot-slide-right-distinct".into(), false)));
    assert!(names.contains(&("dot-slide-left-distinct".into(), false)));
    // Restricted to the lower two layers, only the lower pair matches.
    let low = rewrite::find_matches(
        &c,
        &rules,
        w,
        Some(&Rect {
            layers: (0, 2),
            strands: (0, 2),
        }),
    );
    assert!(low.iter().all(|m| m.occurrence.iter().all(|&i| i < 2)));
    assert!(!low.is_empty());
    // Provenance: the unmatched crossing survives in every result.
    let m = &low[0];
    for s in rewrite::apply_match(&c, &rules[m.rule], m) {
        assert!(s.origin.contains(&Some(2)));
        assert_eq!(
            s.origin.iter().filter(|o| o.is_none()).count(),
            s.word.layers.len() - 1
        );
    }
}

#[test]
fn far_apart_layers_are_brought_together() {
    // psi_1 and psi_3 commute; the dot on strand 4 sits between two crossings it is not part of.
    let c = ctx("klr type=A3 nu=i+j+k+i", Integers);
    let rules = rewrite::rules(&c).unwrap();
    let t = c.parse_term("psi_3 psi_1 x_4 psi_1 psi_3 e(ijki)").unwrap();
    let w = &t.terms[0].0;
    let ms = rewrite::find_matches(&c, &rules, w, None);
    // The two psi_1 form a double crossing once the dot and the psi_3 move out of the way.
    assert!(
        ms.iter()
            .any(|m| rules[m.rule].relation == "square-distinct"
                && m.occurrence.iter().all(|&i| w.layers[i].pos == 0))
    );
}
