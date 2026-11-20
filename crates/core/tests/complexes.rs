//! Complexes of right projectives over weighted KLR algebras (`docs/complexes.md`).

use diagcalc_core::ops;
use serde_json::{Value, json};

/// One black strand right of the red strand `2i`: `e A e = k[x]/(x^2)`, `x` of degree 2.
const CYC: &str = "klrw type=A1 ring=QQ red=\"2i@0\" quotient=cyclotomic";
/// The same without the quotient: `e A e = k[x]`.
const FREE: &str = "klrw type=A1 ring=QQ red=\"2i@0\"";

fn call(op: &str, req: Value) -> Value {
    let v = ops::call(op, &req);
    assert_eq!(v["status"], "ok", "{op}: {v:#}");
    v
}

fn p(at: i64, shift: i64) -> Value {
    json!({ "at": at, "summands": [{ "object": "i@1", "shift": shift }] })
}

/// `P⟨2⟩ --x--> P`, the start of a resolution of the simple module.
fn two_term() -> Value {
    json!({
        "terms": [p(-1, 2), p(0, 0)],
        "differentials": [{ "from": -1, "matrix": [["x_1 e(i@1)"]] }],
    })
}

#[test]
fn hom_bases() {
    let v = call(
        "hom.basis",
        json!({ "context": CYC, "source": "i@1", "target": "e(i@1)" }),
    );
    assert_eq!(
        v["degrees"],
        json!([
            { "degree": 0, "dimension": 1, "basis": ["e(i@1)"] },
            { "degree": 2, "dimension": 1, "basis": ["x_1 e(i@1)"] },
        ]),
        "{v:#}"
    );
    assert_eq!(v["window"]["truncated"], false);
    // left of the red strand the idempotent is violating: zero in the quotient
    let v = call(
        "hom.basis",
        json!({ "context": CYC, "source": "i@-1", "target": "i@-1" }),
    );
    assert_eq!(v["degrees"], json!([]), "{v:#}");
    // without the quotient the space is unbounded, and the cut is reported
    let v = call(
        "hom.basis",
        json!({ "context": FREE, "source": "i@1", "target": "i@1", "max_degree": 4 }),
    );
    assert_eq!(v["degrees"].as_array().unwrap().len(), 3, "{v:#}");
    assert_eq!(v["window"]["truncated"], true);
    assert!(
        v["provenance"]["notes"][0]
            .as_str()
            .unwrap()
            .contains("max_degree")
    );
    let v = call(
        "hom.basis",
        json!({ "context": FREE, "source": "i@1", "target": "i@1", "degree": 6 }),
    );
    assert_eq!(v["degrees"][0]["basis"], json!(["x_1^3 e(i@1)"]), "{v:#}");
}

#[test]
fn complex_checks() {
    let v = call(
        "complex.check",
        json!({ "context": CYC, "complex": two_term() }),
    );
    assert_eq!(v["holds"], true, "{v:#}");
    assert_eq!(v["conventions"]["indexing"], "cohomological");
    // x ∘ x vanishes only in the quotient
    let three = json!({
        "terms": [p(-2, 4), p(-1, 2), p(0, 0)],
        "differentials": [
            { "from": -2, "matrix": [["x_1 e(i@1)"]] },
            { "from": -1, "matrix": [["x_1 e(i@1)"]] },
        ],
    });
    let v = call("complex.check", json!({ "context": CYC, "complex": three }));
    assert_eq!(v["holds"], true, "{v:#}");
    let v = call(
        "complex.check",
        json!({ "context": FREE, "complex": three }),
    );
    assert_eq!(v["holds"], false, "{v:#}");
    assert_eq!(v["failures"][0]["condition"], "d^2", "{v:#}");
    assert_eq!(v["failures"][0]["detail"], "x_1^2 e(i@1)", "{v:#}");
    // a degree-zero map P⟨0⟩ → P cannot be x
    let bad = json!({
        "terms": [p(-1, 0), p(0, 0)],
        "differentials": [{ "from": -1, "matrix": [["x_1 e(i@1)"]] }],
    });
    let v = call("complex.check", json!({ "context": CYC, "complex": bad }));
    assert_eq!(v["failures"][0]["condition"], "degree", "{v:#}");
    // a matrix of the wrong shape is refused when parsed
    let v = ops::call(
        "complex.check",
        &json!({ "context": CYC, "complex": {
            "terms": [p(-1, 2), p(0, 0)],
            "differentials": [{ "from": -1, "matrix": [["x_1 e(i@1)", null]] }],
        }}),
    );
    assert_eq!(v["status"], "error", "{v:#}");
    // complexes live in weighted KLR contexts
    let v = ops::call(
        "complex.check",
        &json!({ "context": "klr type=A1 nu=i", "complex": two_term() }),
    );
    assert_eq!(v["kind"], "precondition", "{v:#}");
}

#[test]
fn a_commuting_square() {
    let at = |x: i64, y: i64, s: i64| json!({ "at": [x, y], "summands": [{ "object": "i@1", "shift": s }] });
    let square = |squares: &str| {
        json!({
            "squares": squares,
            "terms": [at(0, 0, 2), at(1, 0, 0), at(0, 1, 2), at(1, 1, 0)],
            "differentials": [
                { "from": [0, 0], "axis": 0, "matrix": [["x_1 e(i@1)"]] },
                { "from": [0, 1], "axis": 0, "matrix": [["x_1 e(i@1)"]] },
                { "from": [0, 0], "axis": 1, "matrix": [["e(i@1)"]] },
                { "from": [1, 0], "axis": 1, "matrix": [["e(i@1)"]] },
            ],
        })
    };
    let v = call(
        "complex.check",
        json!({ "context": CYC, "complex": square("commute") }),
    );
    assert_eq!(v["holds"], true, "{v:#}");
    let v = call(
        "complex.check",
        json!({ "context": CYC, "complex": square("anticommute") }),
    );
    assert_eq!(v["failures"][0]["condition"], "square", "{v:#}");
    // homology along axis 1: each column P⟨s⟩ --1--> P⟨s⟩ is exact
    let v = call(
        "complex.homology",
        json!({ "context": CYC, "complex": square("commute"), "axis": 1 }),
    );
    assert_eq!(v["exact"], true, "{v:#}");
}

#[test]
fn chain_maps_and_homotopies() {
    let id = json!([{ "at": -1, "matrix": [["e(i@1)"]] }, { "at": 0, "matrix": [["e(i@1)"]] }]);
    let v = call(
        "chainmap.check",
        json!({ "context": CYC, "source": two_term(), "target": two_term(), "map": id }),
    );
    assert_eq!(v["holds"], true, "{v:#}");
    let half = json!([{ "at": 0, "matrix": [["e(i@1)"]] }]);
    let v = call(
        "chainmap.check",
        json!({ "context": CYC, "source": two_term(), "target": two_term(), "map": half }),
    );
    assert_eq!(v["holds"], false, "{v:#}");
    assert_eq!(v["failures"][0]["condition"], "chain map", "{v:#}");
    // P --1--> P is contractible: 1 = d h + h d with h the identity backwards
    let cone = json!({
        "terms": [p(-1, 0), p(0, 0)],
        "differentials": [{ "from": -1, "matrix": [["e(i@1)"]] }],
    });
    let id = json!([{ "at": -1, "matrix": [["e(i@1)"]] }, { "at": 0, "matrix": [["e(i@1)"]] }]);
    let h = json!([{ "at": 0, "matrix": [["e(i@1)"]] }]);
    let v = call(
        "homotopy.check",
        json!({ "context": CYC, "source": cone, "target": cone, "f": id, "h": h }),
    );
    assert_eq!(v["holds"], true, "{v:#}");
    let v = call(
        "homotopy.check",
        json!({ "context": CYC, "source": cone, "target": cone, "f": id, "h": [] }),
    );
    assert_eq!(v["failure_count"], 2, "{v:#}");
}

#[test]
fn homology_of_the_two_term_complex() {
    let v = call(
        "complex.homology",
        json!({ "context": CYC, "complex": two_term(), "representatives": 1 }),
    );
    let ps = &v["positions"];
    // at -1: the kernel x P⟨2⟩, in degree 4; at 0: the simple module, in degree 0
    assert_eq!(
        ps[0]["pieces"],
        json!([{ "test": "i@1", "degree": 4, "dimension": 1, "representatives": [["x_1 e(i@1)"]] }]),
        "{v:#}"
    );
    assert_eq!(
        ps[1]["pieces"],
        json!([{ "test": "i@1", "degree": 0, "dimension": 1, "representatives": [["e(i@1)"]] }]),
        "{v:#}"
    );
    assert_eq!(v["exact"], false);
    // homological indexing: the same complex with d from 1 to 0
    let hom = json!({
        "indexing": "homological",
        "terms": [p(1, 2), p(0, 0)],
        "differentials": [{ "from": 1, "matrix": [["x_1 e(i@1)"]] }],
    });
    let v = call(
        "complex.homology",
        json!({ "context": CYC, "complex": hom, "at": 0 }),
    );
    assert_eq!(v["positions"][0]["pieces"][0]["degree"], 0, "{v:#}");
    // without the quotient x is injective, and the search is cut off and says so
    let v = call(
        "complex.homology",
        json!({ "context": FREE, "complex": two_term(), "at": -1 }),
    );
    assert_eq!(v["positions"][0]["zero"], true, "{v:#}");
    assert_eq!(v["positions"][0]["window"]["truncated"], true, "{v:#}");
    assert_eq!(v["exact"], false, "{v:#}");
    // a sequence that is not a complex is refused
    let bad = json!({
        "terms": [p(-1, 0), p(0, 0)],
        "differentials": [{ "from": -1, "matrix": [["x_1 e(i@1)"]] }],
    });
    let v = ops::call(
        "complex.homology",
        &json!({ "context": CYC, "complex": bad }),
    );
    assert_eq!(v["kind"], "invalid", "{v:#}");
}

#[test]
fn a_resolution_step_by_step() {
    // resolve the simple module: start from P⟨2⟩ --x--> P and add kernels
    let mut c = two_term();
    for step in 0..3 {
        let at = -1 - step;
        let v = call(
            "complex.kernel",
            json!({ "context": CYC, "complex": c, "at": at }),
        );
        assert_eq!(v["complete"], true, "{v:#}");
        assert_eq!(
            v["summands"],
            json!([{ "object": "i@1", "shift": 4 + 2 * step }]),
            "{v:#}"
        );
        assert_eq!(v["matrix"], json!([["x_1 e(i@1)"]]), "{v:#}");
        c = v["extended"].clone();
        let h = call(
            "complex.homology",
            json!({ "context": CYC, "complex": c, "at": at }),
        );
        assert_eq!(h["positions"][0]["zero"], true, "{h:#}");
    }
    let v = call("complex.check", json!({ "context": CYC, "complex": c }));
    assert_eq!(v["holds"], true, "{v:#}");
}
