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
