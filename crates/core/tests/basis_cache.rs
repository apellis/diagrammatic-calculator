//! The cache of basis evaluations (`rep::basis_cache`) of the KL and BE2 normal forms: a repeat in
//! the same hom space hits it, with the same result as without it, and a change to anything the
//! evaluations depend on misses it. One test only, so that no other test of this binary uses the
//! cache at the same time.

use diagcalc_core::context::{Context, ContextSpec};
use diagcalc_core::rep::{self, basis_cache};
use diagcalc_core::ring::{Integers, PrimeField, Rationals, Ring};

fn ctx<R: Ring>(spec: &str, base: R) -> Context<R> {
    let spec = ContextSpec::parse(spec).unwrap();
    let pres = diagcalc_core::builtin::presentation(&spec.presentation).unwrap();
    Context::new(pres, spec, base).unwrap()
}

/// The normal form by the first basis strategy, with the cache's (hits, misses) on the way.
fn nf<R: Ring>(c: &Context<R>, s: &str) -> Outcome {
    let ev = rep::evaluators(c).unwrap().remove(0);
    let t = c.parse_term(s).unwrap();
    let (h0, m0) = basis_cache::stats();
    let out = c.fmt_term(&ev.normal_form(&t).unwrap());
    let (h1, m1) = basis_cache::stats();
    (out, (h1 - h0, m1 - m0))
}

type Outcome = (String, (u64, u64));

fn fingerprint<R: Ring>(c: &Context<R>) -> std::sync::Arc<str> {
    basis_cache::fingerprint("kl", "kl-basis", c)
}

#[test]
fn repeats_hit_and_every_dependency_misses() {
    basis_cache::clear();
    let base = ctx("kl type=A1 mu=-2", Rationals);
    let term = "x_2 psi_1 e(i i)";
    let (first, (h, m)) = nf(&base, term);
    assert_eq!((h, m), (0, 1), "first evaluation");
    // a fresh evaluator and a fresh context with the same data hit, with the same result
    let (again, (h, m)) = nf(&ctx("kl type=A1 mu=-2", Rationals), term);
    assert_eq!((again.as_str(), h, m), (first.as_str(), 1, 0), "repeat");
    // another term of the same hom space and degree hits
    let (other, (h, m)) = nf(&base, "psi_1 x_1 e(i i)");
    assert_eq!((h, m), (1, 0), "same hom space: {other}");
    // and agrees with an evaluation without the cache
    basis_cache::clear();
    let (fresh, (h, m)) = nf(&base, "psi_1 x_1 e(i i)");
    assert_eq!((fresh, h, m), (other, 0, 1));
    let _ = nf(&base, term);

    // each of these differs from the base in one thing the evaluation depends on
    let misses: Vec<(&str, Outcome)> = vec![
        ("degree", nf(&base, "x_2 x_2 psi_1 e(i i)")),
        ("source and target", nf(&base, "x_2 psi_1 e(i i i)")),
        ("mu", nf(&ctx("kl type=A1 mu=-1", Rationals), term)),
        (
            "Cartan datum",
            nf(&ctx("kl type=A2 mu=-2i+j", Rationals), term),
        ),
        (
            "scalars",
            nf(&ctx("kl type=A1 mu=-2 scalars=t", Rationals), term),
        ),
        ("ring ZZ", nf(&ctx("kl type=A1 mu=-2", Integers), term)),
        (
            "ring GF(7)",
            nf(&ctx("kl type=A1 mu=-2", PrimeField::new(7).unwrap()), term),
        ),
        (
            "ring GF(11)",
            nf(&ctx("kl type=A1 mu=-2", PrimeField::new(11).unwrap()), term),
        ),
        ("presentation", {
            let mut c = ctx("kl type=A1 mu=-2", Rationals);
            c.pres.relations.pop();
            nf(&c, term)
        }),
    ];
    for (what, (_, (h, m))) in &misses {
        assert_eq!((*h, *m), (0, 1), "{what} must not hit the cache");
    }
    // the base still hits
    assert_eq!(nf(&base, term).1, (1, 0));

    // the fingerprint covers every part of the context, parameters and weights included
    let f = fingerprint(&base);
    let mut c = base.clone();
    let five = c.params_ring.int(5);
    c.params.entry("Q".into()).or_default().insert((0, 0), five);
    assert_ne!(fingerprint(&c), f, "parameters");
    let mut c = base.clone();
    c.mu = Some(vec![4]);
    assert_ne!(fingerprint(&c), f, "mu");
    let mut c = base.clone();
    c.colours[0].odd = true;
    assert_ne!(fingerprint(&c), f, "parities");
    let mut c = base.clone();
    c.cartan.as_mut().unwrap().a[0][0] = 3;
    assert_ne!(fingerprint(&c), f, "Cartan matrix");
    let mut c = base.clone();
    c.spec.degree_cap = Some(3);
    assert_ne!(fingerprint(&c), f, "specification");
    assert_ne!(
        basis_cache::fingerprint("skm", "kl-basis", &base),
        f,
        "strategy"
    );
    assert_eq!(fingerprint(&base.clone()), f);

    // the BE2 normal form uses the same cache
    let be2 = ctx("be2 type=A1 odd=i mu=-1", Rationals);
    let (a, (h, m)) = nf(&be2, "x_1 psi_1 e(i i)");
    assert_eq!((h, m), (0, 1));
    let (b, (h, m)) = nf(&be2, "x_1 psi_1 e(i i)");
    assert_eq!((a, h, m), (b, 1, 0));
    let even = ctx("be2 type=A1 mu=-1", Rationals);
    let ev = rep::evaluators(&even).unwrap();
    let skm = ev
        .iter()
        .find(|e| e.provenance().method == "skm-basis")
        .unwrap();
    let (h0, m0) = basis_cache::stats();
    skm.normal_form(&even.parse_term("x_1 psi_1 e(i i)").unwrap())
        .unwrap();
    let (h1, m1) = basis_cache::stats();
    assert_eq!((h1 - h0, m1 - m0), (0, 1), "parities of the colours");
}
