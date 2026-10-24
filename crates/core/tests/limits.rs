//! Input limits: a context, term, or Lean export taken from a shared document, an imported file, or
//! a caller cannot make the engine run away, overflow its stack, or write outside the
//! export directory or into the generated Lean code.

use diagcalc_core::lean;
use diagcalc_core::ops;
use serde_json::{Value, json};

fn call(op: &str, req: Value) -> Value {
    ops::call(op, &req)
}

fn error_kind(v: &Value) -> &str {
    assert_eq!(v["status"], "error", "expected an error, got {v}");
    v["kind"].as_str().unwrap()
}

#[test]
fn weights_and_ranks_are_bounded() {
    for ctx in [
        "klr type=A2 nu=10000i+j",
        "klr type=A2 nu=65i",
        "klr type=A2 nu=99999999999999999999i+j",
        "klr type=A100000 nu=i",
        "klr type=A20xA20 nu=i",
        // within the strand limit, but too many sequences of colours
        "klr type=A4 nu=8i+8j+8k+8l",
    ] {
        let v = call("context.describe", json!({ "context": ctx }));
        assert_eq!(error_kind(&v), "limit", "{ctx}: {v}");
    }
    // everyday sizes are untouched
    let v = call(
        "context.describe",
        json!({ "context": "klr type=A3 nu=2i+2j+k" }),
    );
    assert_eq!(v["status"], "ok", "{v}");
}

#[test]
fn deep_nesting_is_a_syntax_error_not_a_crash() {
    let deep = format!("{}x_1 e(i){}", "(".repeat(5000), ")".repeat(5000));
    let v = call(
        "simplify",
        json!({ "context": "klr type=A1 nu=i", "term": deep }),
    );
    assert_eq!(error_kind(&v), "syntax", "{v}");
    assert!(v["message"].as_str().unwrap().contains("nested"));
    // a hundred levels still work
    let ok = format!("{}x_1 e(i){}", "(".repeat(100), ")".repeat(100));
    let v = call(
        "simplify",
        json!({ "context": "klr type=A1 nu=i", "term": ok }),
    );
    assert_eq!(v["status"], "ok", "{v}");
    // and so does a long flat sum
    let flat = vec!["x_1 e(i)"; 2000].join(" + ");
    let v = call(
        "simplify",
        json!({ "context": "klr type=A1 nu=i", "term": flat }),
    );
    assert_eq!(v["status"], "ok", "{v}");
}

#[test]
fn long_exponent_expressions_are_refused() {
    let e = vec!["1"; 2000].join("+");
    let v = call(
        "simplify",
        json!({ "context": "klr type=A1 nu=i", "term": format!("x_1^{{{e}}} e(i)") }),
    );
    assert_eq!(v["status"], "error", "{v}");
}

#[test]
fn lean_names_are_checked_and_docs_escaped() {
    for bad in ["x/../../escaped", "y\n#eval 1", "", "1abc", "a.b", "a-b"] {
        assert!(lean::check_name(bad).is_err(), "{bad:?}");
    }
    for good in ["diagcalc_chain", "A", "psi_twice_2"] {
        lean::check_name(good).unwrap();
    }
    let v = call(
        "export.lean",
        json!({ "context": "klr type=A2 nu=i+j", "term": "psi_1 x_2^2 psi_1 e(ij)", "name": "y\nopen IO in\n#eval IO.println \"injected\"" }),
    );
    assert_eq!(error_kind(&v), "invalid", "{v}");
    let v = call(
        "export.lean",
        json!({ "context": "klr type=A2 nu=i+j", "term": "psi_1 x_2^2 psi_1 e(ij)", "name": "ok_name", "doc": "probe -/ #eval 1 /- x" }),
    );
    assert_eq!(v["status"], "ok", "{v}");
    let text = v["lean"].as_str().unwrap();
    assert!(text.contains("/-- probe - / #eval 1 / - x"), "{text}");
    assert_eq!(text.matches("-/").count(), text.matches("/-").count());
}

#[test]
fn chain_batches_stay_inside_their_directory() {
    let entry = |name: &str| {
        format!(
            "[[chain]]\nname = {name:?}\ncontext = \"klr type=A2 nu=i+j\"\nlhs = \"psi_1 x_2^2 psi_1 e(ij)\"\n"
        )
    };
    assert!(lean::export_batch(&entry("x/../../escaped"), "DiagcalcChains").is_err());
    assert!(lean::export_batch(&entry("y\n#eval 1"), "DiagcalcChains").is_err());
    assert!(lean::export_batch(&entry("ok"), "../Root").is_err());
    let twice = format!("{}{}", entry("a_b"), entry("aB"));
    assert!(lean::export_batch(&twice, "DiagcalcChains").is_err());
    let lhs_with_newline = "[[chain]]\nname = \"ok\"\ncontext = \"klr type=A2 nu=i+j\"\nlhs = \"psi_1 x_2^2 psi_1\\n e(ij)\"\n";
    if let Ok(files) = lean::export_batch(lhs_with_newline, "DiagcalcChains") {
        for (path, text) in files {
            assert!(path == "DiagcalcChains.lean" || path.starts_with("DiagcalcChains/"));
            assert!(text.lines().all(|l| !l.starts_with(" e(ij)")), "{text}");
        }
    }
}
