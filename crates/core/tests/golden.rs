//! The golden corpus (`tests/golden/`): normal forms and polynomial actions computed in Lean by
//! `lean/golden/Golden.lean`. The engine must reproduce the normal form text and the action of
//! every term and of its normal form exactly.

use diagcalc_core::builtin;
use diagcalc_core::context::{Context, ContextSpec};
use diagcalc_core::poly::Monomial;
use diagcalc_core::rep;
use diagcalc_core::ring::{Integers, Ring};
use num_bigint::BigInt;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;

struct Case {
    id: String,
    context: String,
    term: String,
    nf: String,
}

fn read_cases(text: &str) -> Vec<Case> {
    let mut out: Vec<Case> = Vec::new();
    for line in text.lines().map(str::trim) {
        if let Some(id) = line.strip_prefix("case ") {
            out.push(Case {
                id: id.into(),
                context: String::new(),
                term: String::new(),
                nf: String::new(),
            });
        } else if let Some(c) = line.strip_prefix("context ") {
            out.last_mut().unwrap().context = c.into();
        } else if let Some(t) = line.strip_prefix("term ") {
            out.last_mut().unwrap().term = t.into();
        } else if let Some(t) = line.strip_prefix("nf ") {
            out.last_mut().unwrap().nf = t.into();
        }
    }
    out
}

type Action = BTreeMap<String, BTreeMap<Vec<i64>, BigInt>>;

fn lean_output(v: &Value) -> Action {
    let mut out = Action::new();
    for comp in v.as_array().unwrap() {
        let poly = out
            .entry(comp["target"].as_str().unwrap().to_string())
            .or_default();
        for t in comp["poly"].as_array().unwrap() {
            let m: Vec<i64> = t[0]
                .as_array()
                .unwrap()
                .iter()
                .map(|e| e.as_i64().unwrap())
                .collect();
            poly.insert(m, BigInt::from(t[1].as_i64().unwrap()));
        }
    }
    out
}

#[test]
fn golden_corpus_is_reproduced() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/golden");
    let cases = read_cases(&std::fs::read_to_string(root.join("cases.txt")).unwrap());
    let lean: Vec<Value> =
        serde_json::from_str(&std::fs::read_to_string(root.join("actions.json")).unwrap()).unwrap();
    assert!(cases.len() >= 12);
    assert_eq!(
        cases.len(),
        lean.len(),
        "actions.json is stale: regenerate it in Lean"
    );
    let mut checked = 0;
    for (case, l) in cases.iter().zip(&lean) {
        assert_eq!(l["id"], case.id.as_str());
        assert_eq!(
            l["term"],
            case.term.as_str(),
            "{}: actions.json is stale",
            case.id
        );
        assert_eq!(
            l["normal_form"],
            case.nf.as_str(),
            "{}: actions.json is stale",
            case.id
        );
        assert_eq!(
            l["agree"], true,
            "{}: Lean found the normal form wrong",
            case.id
        );
        let spec = ContextSpec::parse(&format!("{} ring=ZZ", case.context)).unwrap();
        let ctx = Context::new(
            builtin::presentation(&spec.presentation).unwrap(),
            spec,
            Integers,
        )
        .unwrap();
        let ev = rep::evaluators(&ctx).unwrap().remove(0);
        let term = ctx.parse_term(&case.term).unwrap();
        let nf = ctx.parse_term(&case.nf).unwrap();
        assert_eq!(
            ctx.fmt_term(&ev.normal_form(&term).unwrap()),
            case.nf,
            "{}: normal form",
            case.id
        );
        for act in l["actions"].as_array().unwrap() {
            let src = act["source"].as_str().unwrap();
            let obj = ctx
                .objects
                .iter()
                .find(|o| ctx.fmt_object(o) == src)
                .unwrap_or_else(|| panic!("{src}"));
            let n = obj.len();
            let ring = ev.poly_ring(n);
            let mut m = vec![0i32; ring.nvars()];
            for (k, e) in act["input"].as_array().unwrap().iter().enumerate() {
                m[k] = e.as_i64().unwrap() as i32;
            }
            let f = ring.monomial(Monomial::from(m), ring.base.one());
            let want = lean_output(&act["output"]);
            for (what, t) in [("term", &term), ("normal form", &nf)] {
                let mut got = Action::new();
                for (o, p) in ev.act(t, obj, &f).unwrap() {
                    let poly = got.entry(ctx.fmt_object(&o)).or_default();
                    for (mono, c) in &p.terms {
                        poly.insert(
                            mono.0[..n].iter().map(|&e| e as i64).collect(),
                            c.to_bigint(),
                        );
                    }
                }
                assert_eq!(
                    got, want,
                    "{}: action of the {what} at e({src}) on {:?}",
                    case.id, act["input"]
                );
                checked += 1;
            }
        }
    }
    assert!(checked > 500, "{checked}");
}
