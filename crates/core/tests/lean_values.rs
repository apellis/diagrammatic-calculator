//! The engine reproduces the Lean regression values (`tests/lean-values/values.json`), computed
//! by `lean/golden/LeanValues.lean` from theorems in the Lean libraries. Every row names its
//! source theorem.

use diagcalc_core::builtin;
use diagcalc_core::context::{Context, ContextSpec};
use diagcalc_core::quotient::Quotient;
use diagcalc_core::rep;
use diagcalc_core::ring::{Rationals, Ring};
use diagcalc_core::term::Object;
use diagcalc_core::toolkit::{self, Ambient};
use serde_json::Value;

fn ctx<R: Ring>(spec: &str, base: R) -> Context<R> {
    let spec = ContextSpec::parse(spec).unwrap();
    let pres = builtin::presentation(&spec.presentation).unwrap();
    Context::new(pres, spec, base).unwrap()
}

fn rows() -> Vec<Value> {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/lean-values/values.json"
    );
    let v: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(v["format"], "diagcalc-lean-values/1");
    v["rows"].as_array().unwrap().clone()
}

fn object<R: Ring>(c: &Context<R>, name: &str) -> Object {
    c.objects
        .iter()
        .find(|o| c.fmt_object(o) == name)
        .unwrap_or_else(|| panic!("no object {name}"))
        .clone()
}

fn dims(row: &Value) -> Vec<(i64, usize)> {
    row["dims"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(d, n)| (d.parse().unwrap(), n.as_u64().unwrap() as usize))
        .collect()
}

/// Graded dimensions of `R(ν) e(i)` and its corners `e(j) R(ν) e(i)`: `KL1.gdim_cornerGrade`,
/// `KL2.gdim_cornerGrade2`, `NilHeckeGrading.degree_finrank_binomial`.
#[test]
fn corner_dimensions() {
    let mut checked = 0;
    for row in rows() {
        let kind = row["kind"].as_str().unwrap();
        if kind != "corner" && kind != "algebra" {
            continue;
        }
        let c = ctx(row["context"].as_str().unwrap(), Rationals);
        let evs = rep::evaluators(&c).unwrap();
        let src = object(&c, row["source_object"].as_str().unwrap());
        let tgt = row["target_object"].as_str().map(|t| object(&c, t));
        for ev in &evs {
            for (d, n) in dims(&row) {
                let basis = ev.graded_basis(&src, d).unwrap();
                let count = basis
                    .iter()
                    .filter(|w| tgt.as_ref().is_none_or(|t| c.target(w) == *t))
                    .count();
                assert_eq!(count, n, "{} in degree {d}", row["id"]);
            }
        }
        checked += 1;
    }
    assert!(checked > 40, "only {checked} rows");
}

/// The centre of `R(ν)` degree by degree: `GradingDatum.gdim_center`.
#[test]
fn centre_dimensions() {
    let mut checked = 0;
    for row in rows() {
        if row["kind"] != "centre" {
            continue;
        }
        let c = ctx(row["context"].as_str().unwrap(), Rationals);
        let evs = rep::evaluators(&c).unwrap();
        let alg = Ambient {
            ctx: &c,
            eval: evs[0].as_ref(),
        };
        for (d, n) in dims(&row) {
            assert_eq!(
                toolkit::centre(&alg, d).unwrap().len(),
                n,
                "{} in degree {d}",
                row["id"]
            );
        }
        checked += 1;
    }
    assert_eq!(checked, 6);
}

/// Ranks of cyclotomic quotients: `CycKLR.finrank_corner` (corners of a multiplicity-free KLR
/// quotient) and `Cyclotomic.prop_5_2` with `OddGrassmannSchur.finrank_OH` (odd nilHecke).
#[test]
fn cyclotomic_ranks() {
    let mut checked = 0;
    for row in rows() {
        if row["kind"] != "rank" {
            continue;
        }
        let rank = row["rank"].as_u64().unwrap() as usize;
        let spec = row["context"].as_str().unwrap();
        if spec.starts_with("odd-nilhecke") {
            // No Cartan datum here, so no degree bound is known in advance: compute up to the top
            // degree a(a−1) + 2a(N−a) of the even analogue. The computed pieces are exact, so
            // reaching the full rank shows nothing lies above.
            let c0 = ctx(spec, Rationals);
            let a = c0.objects[0].len() as i64;
            let n: i64 = c0.spec.lambda.as_ref().unwrap().parse().unwrap();
            let cap = a * (a - 1) + 2 * a * (n - a);
            let c = ctx(&format!("{spec} degcap={cap}"), Rationals);
            for ev in rep::evaluators(&c).unwrap() {
                let q = Quotient::from_context(&c, ev).unwrap();
                let total: usize = q
                    .graded_dimension(&c.objects[0], None)
                    .unwrap()
                    .values()
                    .sum();
                assert_eq!(total, rank, "{}", row["id"]);
            }
        } else {
            let c = ctx(spec, Rationals);
            let src = object(&c, row["source_object"].as_str().unwrap());
            let tgt = object(&c, row["target_object"].as_str().unwrap());
            for ev in rep::evaluators(&c).unwrap() {
                let q = Quotient::from_context(&c, ev).unwrap();
                let total: usize = q.graded_dimension(&src, Some(&tgt)).unwrap().values().sum();
                assert_eq!(total, rank, "{}", row["id"]);
            }
        }
        checked += 1;
    }
    assert_eq!(checked, 9);
}
