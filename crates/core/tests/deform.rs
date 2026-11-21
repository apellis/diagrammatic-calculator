//! Coefficients in `k[h]/(h^{N+1})` and the order-by-order solver (`docs/complexes.md`,
//! "Parameters and deformations").

use diagcalc_core::context::ContextSpec;
use diagcalc_core::ops;
use serde_json::{Value, json};

/// `e A e = k[x]`, `x` of degree 2, with a parameter `h` of degree 2 kept to order `N`.
fn ctx(n: u32) -> String {
    format!("klrw type=A1 ring=QQ red=\"2i@0\" truncate=h^{n} hdeg=2")
}

fn call(op: &str, req: Value) -> Value {
    let v = ops::call(op, &req);
    assert_eq!(v["status"], "ok", "{op}: {v:#}");
    v
}

fn p(at: i64, shift: i64) -> Value {
    json!({ "at": at, "summands": [{ "object": "i@1", "shift": shift }] })
}

/// `P⟨4⟩ --a--> P⟨2⟩ --b--> P`.
fn three_term(a: &str, b: &str) -> Value {
    json!({
        "terms": [p(-2, 4), p(-1, 2), p(0, 0)],
        "differentials": [
            { "from": -2, "matrix": [[a]] },
            { "from": -1, "matrix": [[b]] },
        ],
    })
}

#[test]
fn context_keys_round_trip() {
    let s = ContextSpec::parse(&ctx(3)).unwrap();
    assert_eq!(s.truncate, Some(("h".to_string(), 3)));
    assert_eq!(s.param_degree, Some(2));
    assert!(s.scalars.iter().any(|x| x == "h"));
    let again = ContextSpec::parse(&s.print()).unwrap();
    assert_eq!(again.truncate, s.truncate);
    assert_eq!(again.param_degree, s.param_degree);
    assert!(ContextSpec::parse("klrw type=A1 ring=QQ red=\"2i@0\" hdeg=2").is_err());
}

#[test]
fn hom_bases_count_powers_of_h() {
    let v = call(
        "hom.basis",
        json!({ "context": ctx(1), "source": "i@1", "target": "i@1", "max_degree": 4 }),
    );
    assert_eq!(
        v["degrees"],
        json!([
            { "degree": 0, "dimension": 1, "basis": ["e(i@1)"] },
            { "degree": 2, "dimension": 2, "basis": ["x_1 e(i@1)", "h e(i@1)"] },
            { "degree": 4, "dimension": 2, "basis": ["x_1^2 e(i@1)", "h x_1 e(i@1)"] },
        ])
    );
    assert_eq!(v["provenance"]["truncation"]["order"], 1);
}

#[test]
fn truncation_decides_d_squared() {
    // (h e)(h e) = h^2 e vanishes modulo h^2 but not modulo h^3
    let c = three_term("h e(i@1)", "h e(i@1)");
    let v = call("complex.check", json!({ "context": ctx(1), "complex": c }));
    assert_eq!(v["holds"], true, "{v:#}");
    let v = call("complex.check", json!({ "context": ctx(2), "complex": c }));
    assert_eq!(v["holds"], false, "{v:#}");
}

#[test]
fn deform_corrects_and_reports_freedom() {
    let v = call(
        "complex.deform",
        json!({
            "context": ctx(2),
            "complex": three_term("h e(i@1)", "x_1 e(i@1)"),
            "unknowns": [
                { "map": "differential", "from": -2 },
                { "map": "differential", "from": -1, "row": 0, "col": 0 },
            ],
        }),
    );
    assert_eq!(v["solved"], true);
    assert_eq!(v["solved_to_order"], 2);
    assert_eq!(v["failures"], json!([]));
    let o1 = &v["orders"][0];
    assert_eq!(o1["order"], 1);
    assert_eq!(
        o1["correction"],
        json!([{ "at": -2, "matrix": [["-h e(i@1)"]] }])
    );
    assert_eq!(o1["freedom_dimension"], 1);
    assert_eq!(
        o1["freedom"],
        json!([[{ "at": -1, "matrix": [["h e(i@1)"]] }]])
    );
    // the corrected complex is one
    let c = call(
        "complex.check",
        json!({ "context": ctx(2), "complex": v["complex"] }),
    );
    assert_eq!(c["holds"], true, "{c:#}");
}

#[test]
fn deform_reports_obstruction_with_order() {
    let v = call(
        "complex.deform",
        json!({ "context": ctx(2), "complex": three_term("h e(i@1)", "x_1 e(i@1)") }),
    );
    assert_eq!(v["solved"], false);
    assert_eq!(v["solved_to_order"], 0);
    let f = &v["failures"][0];
    assert_eq!(f["condition"], "obstruction");
    assert_eq!(f["order"], 1);
    assert!(f["detail"].as_str().unwrap().contains("x_1 e(i@1)"));
    assert_eq!(
        f["maps"],
        json!([
            { "map": "differential", "from": -2, "axis": 0 },
            { "map": "differential", "from": -1, "axis": 0 },
        ])
    );
}

#[test]
fn deform_needs_a_complex_at_order_zero() {
    let v = call(
        "complex.deform",
        json!({
            "context": ctx(1),
            "complex": three_term("x_1 e(i@1)", "x_1 e(i@1)"),
            "unknowns": [{ "map": "differential", "from": -2 }],
        }),
    );
    assert_eq!(v["solved"], false);
    assert_eq!(v["solved_to_order"], Value::Null);
    assert_eq!(v["failures"][0]["order"], 0);
    assert_eq!(v["failures"][0]["condition"], "d^2");
}

#[test]
fn deform_needs_a_truncation() {
    let v = ops::call(
        "complex.deform",
        &json!({
            "context": "klrw type=A1 ring=QQ red=\"2i@0\"",
            "complex": three_term("0", "x_1 e(i@1)"),
        }),
    );
    assert_eq!(v["status"], "error", "{v:#}");
}

/// A₂ with `Q_ij = h`: the crossing bigon `ψ ψ e(ij)` is `h e(ij)`, so two `h`-free maps
/// compose to a multiple of `h`.
const BIGON: &str = "klrw type=A2 ring=QQ Q_ij=h truncate=h^1 hdeg=2";

/// `P(ij)⟨2⟩ --ψ--> P(ji)⟨1⟩ --ψ--> P(ij)`, with `d² = h · id`.
fn bigon_complex(a: &str) -> Value {
    let t = |at: i64, obj: &str, shift: i64| json!({ "at": at, "summands": [{ "object": obj, "shift": shift }] });
    json!({
        "terms": [t(-2, "i@0 j@1", 2), t(-1, "j@0 i@1", 1), t(0, "i@0 j@1", 0)],
        "differentials": [
            { "from": -2, "matrix": [[a]] },
            { "from": -1, "matrix": [["psi_1 e(j@0 i@1)"]] },
        ],
    })
}

#[test]
fn deform_reads_h_from_composites() {
    // with h = 0 it is a complex, though d ∘ d = h e(ij) is not zero as written
    let c = bigon_complex("psi_1 e(i@0 j@1)");
    let ctx0 = "klrw type=A2 ring=QQ Q_ij=0";
    let v = call("complex.check", json!({ "context": ctx0, "complex": c }));
    assert_eq!(v["holds"], true, "{v:#}");
    let v = call("complex.check", json!({ "context": BIGON, "complex": c }));
    assert_eq!(v["holds"], false, "{v:#}");
    // the h in the composite is an obstruction at order 1, not a failure at order 0
    let v = call("complex.deform", json!({ "context": BIGON, "complex": c }));
    assert_eq!(v["solved"], false, "{v:#}");
    assert_eq!(v["solved_to_order"], 0, "{v:#}");
    assert_eq!(v["failures"][0]["order"], 1, "{v:#}");
    assert_eq!(v["failures"][0]["condition"], "obstruction");
}

#[test]
fn deform_corrects_against_h_from_a_composite() {
    // P(ij)⟨2⟩ --(ψ, a)--> P(ji)⟨1⟩ ⊕ P(ij) --(ψ, e)--> P(ij): d² = h e + a, so a = -h e
    let t = |at: i64, s: Value| json!({ "at": at, "summands": s });
    let c = json!({
        "terms": [
            t(-2, json!([{ "object": "i@0 j@1", "shift": 2 }])),
            t(-1, json!([{ "object": "j@0 i@1", "shift": 1 }, { "object": "i@0 j@1", "shift": 0 }])),
            t(0, json!([{ "object": "i@0 j@1", "shift": 0 }])),
        ],
        "differentials": [
            { "from": -2, "matrix": [["psi_1 e(i@0 j@1)"], [null]] },
            { "from": -1, "matrix": [["psi_1 e(j@0 i@1)", "e(i@0 j@1)"]] },
        ],
    });
    let v = call(
        "complex.deform",
        json!({
            "context": BIGON,
            "complex": c,
            "unknowns": [{ "map": "differential", "from": -2, "row": 1, "col": 0 }],
        }),
    );
    assert_eq!(v["solved"], true, "{v:#}");
    assert_eq!(v["solved_to_order"], 1);
    assert_eq!(
        v["orders"][0]["correction"],
        json!([{ "at": -2, "matrix": [[null], ["-h e(i@0 j@1)"]] }])
    );
    let c2 = call(
        "complex.check",
        json!({ "context": BIGON, "complex": v["complex"] }),
    );
    assert_eq!(c2["holds"], true, "{c2:#}");
}

#[test]
fn deform_sees_every_summand_of_the_middle_term() {
    // P → P ⊕ P → P with d₋₂ = (0, 1)ᵀ and d₋₁ = (0, 1): d² = 1 through the second summand of
    // the middle term, so the h-free part is not a complex. The coefficients of d² were once
    // computed with the inner dimension of the product taken from the target (one summand), which
    // dropped the second summand and reported the complex solved.
    let ctx = "klrw type=A1 ring=QQ red=\"i@0\" quotient=cyclotomic truncate=h^2 hdeg=0";
    let s = |at: i64, n: usize| json!({ "at": at, "summands": vec![json!({ "object": "i@1", "shift": 0 }); n] });
    let c = json!({
        "terms": [s(-2, 1), s(-1, 2), s(0, 1)],
        "differentials": [
            { "from": -2, "matrix": [[null], ["e(i@1)"]] },
            { "from": -1, "matrix": [[null, "e(i@1)"]] },
        ],
    });
    let v = call("complex.check", json!({ "context": ctx, "complex": c }));
    assert_eq!(v["holds"], false, "{v:#}");
    let v = call("complex.deform", json!({ "context": ctx, "complex": c }));
    assert_eq!(v["solved"], false, "{v:#}");
    assert_eq!(v["solved_to_order"], Value::Null, "{v:#}");
    assert_eq!(v["failures"][0]["condition"], "d^2", "{v:#}");
    assert_eq!(v["failures"][0]["order"], 0, "{v:#}");
}
