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

fn identity_on(c: &Value) -> Value {
    let mut out = Vec::new();
    for t in c["terms"].as_array().unwrap() {
        let n = t["summands"].as_array().unwrap().len();
        let m: Vec<Vec<&str>> = (0..n)
            .map(|i| {
                (0..n)
                    .map(|j| if i == j { "e(i@1)" } else { "0" })
                    .collect()
            })
            .collect();
        out.push(json!({ "at": t["at"], "matrix": m }));
    }
    Value::Array(out)
}

#[test]
fn lifting_and_extending_maps() {
    let p0 = json!([{ "object": "i@1", "shift": 0 }]);
    let p2 = json!([{ "object": "i@1", "shift": 2 }]);
    let v = call(
        "map.lift",
        json!({ "context": CYC, "source": p2, "middle": p0, "target": p0,
                "f": [["x_1 e(i@1)"]], "g": [["e(i@1)"]] }),
    );
    assert_eq!(v["exists"], true, "{v:#}");
    assert_eq!(v["solution"], json!([["x_1 e(i@1)"]]), "{v:#}");
    let v = call(
        "map.lift",
        json!({ "context": CYC, "source": p0, "middle": p2, "target": p0,
                "f": [["e(i@1)"]], "g": [["x_1 e(i@1)"]] }),
    );
    assert_eq!(v["exists"], false, "{v:#}");
    let v = call(
        "map.extend",
        json!({ "context": CYC, "source": p2, "middle": p0, "target": p0,
                "f": [["x_1 e(i@1)"]], "g": [["x_1 e(i@1)"]] }),
    );
    assert_eq!(v["solution"], json!([["e(i@1)"]]), "{v:#}");
}

#[test]
fn solving_for_chain_maps_and_homotopies() {
    // the identity at 0 extends uniquely to a chain endomorphism
    let v = call(
        "chainmap.solve",
        json!({ "context": CYC, "source": two_term(), "target": two_term(),
                "fixed": [{ "at": 0, "matrix": [["e(i@1)"]] }] }),
    );
    assert_eq!(v["exists"], true, "{v:#}");
    assert_eq!(v["freedom"], 0, "{v:#}");
    assert_eq!(v["map"], identity_on(&two_term()), "{v:#}");
    // the identity of P⟨2⟩ → P is not null-homotopic
    let v = call(
        "homotopy.find",
        json!({ "context": CYC, "source": two_term(), "target": two_term(),
                "f": identity_on(&two_term()) }),
    );
    assert_eq!(v["exists"], false, "{v:#}");
    let v = call(
        "complex.minimal",
        json!({ "context": CYC, "complex": two_term() }),
    );
    assert_eq!(v["holds"], true, "{v:#}");
}

#[test]
fn the_cone_of_the_identity_is_contractible() {
    let id = identity_on(&two_term());
    let v = call(
        "complex.cone",
        json!({ "context": CYC, "source": two_term(), "target": two_term(), "map": id }),
    );
    let cone = v["complex"].clone();
    let v = call("complex.check", json!({ "context": CYC, "complex": cone }));
    assert_eq!(v["holds"], true, "{v:#}");
    let v = call(
        "complex.minimal",
        json!({ "context": CYC, "complex": cone }),
    );
    assert_eq!(v["holds"], false, "{v:#}");
    let v = call(
        "homotopy.find",
        json!({ "context": CYC, "source": cone, "target": cone, "f": identity_on(&cone) }),
    );
    assert_eq!(v["exists"], true, "{v:#}");
    let h = v["h"].clone();
    let v = call(
        "homotopy.check",
        json!({ "context": CYC, "source": cone, "target": cone, "f": identity_on(&cone), "h": h }),
    );
    assert_eq!(v["holds"], true, "{v:#}");
    let v = call("complex.gauss", json!({ "context": CYC, "complex": cone }));
    assert_eq!(v["finished"], true, "{v:#}");
    assert_eq!(v["complex"]["terms"], json!([]), "{v:#}");
    assert_eq!(v["eliminations"].as_array().unwrap().len(), 2, "{v:#}");
    let v = call(
        "complex.hom",
        json!({ "context": CYC, "source": cone, "target": cone }),
    );
    for d in v["degrees"].as_array().unwrap() {
        for h in d["cohomology"].as_array().unwrap() {
            assert_eq!(h["dimension"], 0, "{v:#}");
        }
    }
}

#[test]
fn shifts_totalization_and_euler_characteristic() {
    let v = call(
        "complex.shift",
        json!({ "context": CYC, "complex": two_term(), "by": 1, "grading": 2 }),
    );
    let c = &v["complex"];
    assert_eq!(c["terms"], json!([p(-2, 4), p(-1, 2)]), "{v:#}");
    assert_eq!(
        c["differentials"][0]["matrix"],
        json!([["-x_1 e(i@1)"]]),
        "{v:#}"
    );
    let one = |s: i64| json!([{ "object": "i@1", "shift": s }]);
    let x = json!([["x_1 e(i@1)"]]);
    let square = json!({
        "terms": [
            { "at": [0, 0], "summands": one(2) }, { "at": [1, 0], "summands": one(0) },
            { "at": [0, 1], "summands": one(0) }, { "at": [1, 1], "summands": one(-2) },
        ],
        "differentials": [
            { "from": [0, 0], "axis": 0, "matrix": x }, { "from": [0, 1], "axis": 0, "matrix": x },
            { "from": [0, 0], "axis": 1, "matrix": x }, { "from": [1, 0], "axis": 1, "matrix": x },
        ],
    });
    let v = call(
        "complex.totalize",
        json!({ "context": CYC, "complex": square }),
    );
    let d = &v["complex"]["differentials"];
    assert_eq!(
        d[0]["matrix"],
        json!([["x_1 e(i@1)"], ["x_1 e(i@1)"]]),
        "{v:#}"
    );
    assert_eq!(
        d[1]["matrix"],
        json!([["x_1 e(i@1)", "-x_1 e(i@1)"]]),
        "{v:#}"
    );
    let v = call(
        "complex.check",
        json!({ "context": CYC, "complex": v["complex"] }),
    );
    assert_eq!(v["holds"], true, "{v:#}");
    let v = call(
        "complex.euler",
        json!({ "context": CYC, "complex": square }),
    );
    assert_eq!(v["text"], "(q^2 - 2 + q^-2)[i@1]", "{v:#}");
}

#[test]
fn hom_complex_cohomology() {
    let target = json!({ "terms": [p(0, 0)], "differentials": [] });
    let v = call(
        "complex.hom",
        json!({ "context": CYC, "source": two_term(), "target": target }),
    );
    assert_eq!(
        v["degrees"],
        json!([
            { "degree": -2, "cohomology": [{ "n": 1, "cochains": 1, "dimension": 1 }] },
            { "degree": 2, "cohomology": [{ "n": 0, "cochains": 1, "dimension": 1 }] },
        ]),
        "{v:#}"
    );
}

/// The simple module `L = P/xP` for `P = e(i@1)A` in the cyclotomic quotient.
fn simple() -> Value {
    json!({
        "generators": [{ "object": "i@1", "shift": 0 }],
        "relations": [{ "object": "i@1", "shift": 2 }],
        "matrix": [["x_1 e(i@1)"]],
    })
}

#[test]
fn modules_and_heads() {
    let v = call(
        "module.check",
        json!({ "context": CYC, "module": simple() }),
    );
    assert_eq!(v["holds"], true, "{v:#}");
    assert_eq!(v["dimension"], 1, "{v:#}");
    // a relation of the wrong degree
    let bad = json!({
        "generators": [{ "object": "i@1", "shift": 0 }],
        "relations": [{ "object": "i@1", "shift": 0 }],
        "matrix": [["x_1 e(i@1)"]],
    });
    let v = call("module.check", json!({ "context": CYC, "module": bad }));
    assert_eq!(v["holds"], false, "{v:#}");
    assert!(
        v["failures"][0]["location"]
            .as_str()
            .unwrap()
            .contains("presentation matrix"),
        "{v:#}"
    );
    // P/rad P is the simple module, with x as its one relation
    let v = call("module.head", json!({ "context": CYC, "object": "i@1" }));
    assert_eq!(v["dimension"], 1, "{v:#}");
    assert_eq!(
        v["module"]["relations"].as_array().unwrap().len(),
        1,
        "{v:#}"
    );
    let m = v["module"].clone();
    let v = call("module.check", json!({ "context": CYC, "module": m }));
    assert_eq!(v["dimension"], 1, "{v:#}");
}

#[test]
fn the_head_of_a_local_projective_has_no_simplicity_warning() {
    // `End⁰(P(i@1))/J` is one-dimensional, so the head is simple and there is no warning; the
    // note once reported `dim J` as the dimension of the quotient, which is larger once `h` enters
    let ctx = "klrw type=A1 ring=QQ red=\"2i@0\" quotient=cyclotomic truncate=h^2 hdeg=0";
    let v = call("module.head", json!({ "context": ctx, "object": "i@1" }));
    let notes = v["provenance"].to_string() + &v["notes"].to_string();
    assert!(!notes.contains("may not be simple"), "{v:#}");
    let v = call("module.head", json!({ "context": CYC, "object": "i@1" }));
    assert!(!v.to_string().contains("may not be simple"), "{v:#}");
}

#[test]
fn resolving_the_simple_module() {
    let v = call(
        "resolution.start",
        json!({ "context": CYC, "module": simple() }),
    );
    assert_eq!(v["check"]["holds"], true, "{v:#}");
    assert_eq!(v["check"]["resolved"], false, "{v:#}");
    assert_eq!(v["check"]["next_at"], -1, "{v:#}");
    assert_eq!(v["check"]["uncovered_dimension"], 1, "{v:#}");
    let mut r = v["resolution"].clone();
    for n in 1..=3 {
        let v = call(
            "resolution.extend",
            json!({ "context": CYC, "resolution": r }),
        );
        assert_eq!(v["added"]["at"], -n, "{v:#}");
        assert_eq!(
            v["added"]["summands"],
            json!([{ "object": "i@1", "shift": 2 * n }]),
            "{v:#}"
        );
        assert_eq!(v["check"]["holds"], true, "{v:#}");
        // the kernel of x on k[x]/(x²) is never zero: the resolution goes on forever
        assert_eq!(v["check"]["next_at"], -n - 1, "{v:#}");
        r = v["resolution"].clone();
    }
    let v = call(
        "resolution.check",
        json!({ "context": CYC, "resolution": r }),
    );
    assert_eq!(v["holds"], true, "{v:#}");
    assert_eq!(v["length"], 3, "{v:#}");
    assert_eq!(v["module_dimension"], 1, "{v:#}");
    // an augmentation that does not map into the module correctly: x instead of 1
    let mut bad = r.clone();
    bad["resolves"]["augmentation"] = json!([["x_1 e(i@1)"]]);
    let v = call(
        "resolution.check",
        json!({ "context": CYC, "resolution": bad }),
    );
    assert_eq!(v["holds"], false, "{v:#}");
}

#[test]
fn augmentation_check_sees_every_summand_of_c0() {
    // the free module P, resolved by P --(0, 1)ᵀ--> P ⊕ P with ε = (1, 1): ε ∘ d = 1 ≠ 0 through
    // the second summand of C₀. The composite was once computed with the inner dimension taken
    // from the generators of the module (one), which dropped that summand and found ε ∘ d = 0.
    let p = || json!({ "object": "i@1", "shift": 0 });
    let r = json!({
        "terms": [
            { "at": -1, "summands": [p()] },
            { "at": 0, "summands": [p(), p()] },
        ],
        "differentials": [{ "from": -1, "matrix": [[null], ["e(i@1)"]] }],
        "resolves": {
            "module": { "generators": [p()] },
            "augmentation": [["e(i@1)", "e(i@1)"]],
        },
    });
    let v = call(
        "resolution.check",
        json!({ "context": CYC, "resolution": r }),
    );
    assert_eq!(v["holds"], false, "{v:#}");
    assert_eq!(v["failures"][0]["condition"], "augmentation", "{v:#}");
}

#[test]
fn resolving_a_projective_and_gauss_on_resolutions() {
    // P itself: C₀ = P is already a resolution
    let p = json!({ "generators": [{ "object": "i@1", "shift": 0 }] });
    let v = call("resolution.start", json!({ "context": CYC, "module": p }));
    assert_eq!(v["check"]["resolved"], true, "{v:#}");
    assert_eq!(v["check"]["minimal"], true, "{v:#}");
    // a redundant start for L: P ⊕ P⟨2⟩ → L by (1, x); the kernel of C₀ → L is generated by
    // (x, 0) and (0, 1), both from P⟨2⟩
    let r = json!({
        "terms": [
            { "at": -1, "summands": [{ "object": "i@1", "shift": 2 }] },
            { "at": 0, "summands": [{ "object": "i@1", "shift": 0 }, { "object": "i@1", "shift": 2 }] },
        ],
        "differentials": [{ "from": -1, "matrix": [[null], ["e(i@1)"]] }],
        "resolves": { "module": simple(), "augmentation": [["e(i@1)", "x_1 e(i@1)"]] },
    });
    // only (0, 1) is covered: not exact at 0
    let v = call(
        "resolution.check",
        json!({ "context": CYC, "resolution": r }),
    );
    assert_eq!(v["holds"], false, "{v:#}");
    assert_eq!(v["failures"][0]["condition"], "exact", "{v:#}");
    // as the far end at 0 is covered by both generators, extending from C₀ alone works
    let mut r0 = r.clone();
    r0["terms"] = json!([r["terms"][1]]);
    r0["differentials"] = json!([]);
    let v = call(
        "resolution.extend",
        json!({ "context": CYC, "resolution": r0 }),
    );
    assert_eq!(v["added"]["summands"].as_array().unwrap().len(), 2, "{v:#}");
    let r1 = v["resolution"].clone();
    // Gaussian elimination removes the identity entry and keeps the module and augmentation
    let v = call("complex.gauss", json!({ "context": CYC, "complex": r1 }));
    let g = v["complex"].clone();
    assert_eq!(
        g["resolves"]["augmentation"].as_array().unwrap()[0]
            .as_array()
            .unwrap()
            .len(),
        1,
        "{v:#}"
    );
    let v = call(
        "resolution.check",
        json!({ "context": CYC, "resolution": g }),
    );
    assert_eq!(v["holds"], true, "{v:#}");
    assert_eq!(v["next_at"], -2, "{v:#}");
}

#[test]
fn head_window_is_never_empty_when_max_degree_is_low() {
    // the only degree with maps is 10; a max_degree of 2 is raised to it rather than leaving an
    // empty window and a zero head
    let v = call(
        "module.head",
        json!({
            "context": "klrw type=A1 ring=QQ red=\"2i@0\"",
            "object": "i@1",
            "shift": 10,
            "max_degree": 2,
        }),
    );
    assert_eq!(v["dimension"], 1, "{v:#}");
    assert_eq!(v["dimensions"][0]["powers"], json!([[10, 1]]), "{v:#}");
    assert_eq!(
        v["window"],
        json!({ "from": 10, "to": 10, "truncated": true }),
        "{v:#}"
    );
    assert_eq!(v["complete"], false, "{v:#}");
}

/// Summands with different numbers of black strands: every hom between them is zero, and every
/// operation treats them alike (they used to fail or panic in the strand bookkeeping).
#[test]
fn summands_with_different_strand_counts() {
    let one = json!({ "terms": [{ "at": 0, "summands": [{ "object": "i@1" }] }] });
    let two = json!({ "terms": [{ "at": 0, "summands": [{ "object": "i@1 i@2" }] }] });
    let v = call(
        "hom.basis",
        json!({ "context": CYC, "source": "i@1", "target": "i@1 i@2" }),
    );
    assert_eq!(v["degrees"], json!([]), "{v:#}");
    let v = call(
        "complex.hom",
        json!({ "context": CYC, "source": one, "target": two }),
    );
    assert_eq!(v["degrees"], json!([]), "{v:#}");
    // P(i@1 i@2) ⊕ P(i@1)⟨2⟩ --[0, x]--> P(i@1)
    let c = json!({
        "terms": [
            { "at": -1, "summands": [{ "object": "i@1 i@2" }, { "object": "i@1", "shift": 2 }] },
            p(0, 0),
        ],
        "differentials": [{ "from": -1, "matrix": [[null, "x_1 e(i@1)"]] }],
    });
    for op in ["complex.check", "complex.minimal"] {
        let v = call(op, json!({ "context": CYC, "complex": c }));
        assert_eq!(v["holds"], true, "{op}: {v:#}");
    }
    let v = call(
        "complex.homology",
        json!({ "context": CYC, "complex": c, "at": -1 }),
    );
    // H^{-1} = P(i@1 i@2) ⊕ x P(i@1)⟨2⟩: tested on i@1 i@2 it is End(P(i@1 i@2)) = q^-2 + 2 + q^2,
    // on i@1 the line spanned by x in degree 4
    let pieces: Vec<_> = v["positions"][0]["pieces"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["test"].clone(),
                p["degree"].clone(),
                p["dimension"].clone(),
            )
        })
        .collect();
    assert_eq!(
        pieces,
        vec![
            (json!("i@1 i@2"), json!(-2), json!(1)),
            (json!("i@1 i@2"), json!(0), json!(2)),
            (json!("i@1 i@2"), json!(2), json!(1)),
            (json!("i@1"), json!(4), json!(1)),
        ],
        "{v:#}"
    );
    let v = call(
        "complex.kernel",
        json!({ "context": CYC, "complex": c, "at": -1 }),
    );
    assert_eq!(v["complete"], true, "{v:#}");
    let v = call(
        "complex.hom",
        json!({ "context": CYC, "source": c, "target": c }),
    );
    assert!(!v["degrees"].as_array().unwrap().is_empty(), "{v:#}");
    let v = call("complex.euler", json!({ "context": CYC, "complex": c }));
    assert_eq!(v["text"], "(-1)[i@1 i@2] + (-q^2 + 1)[i@1]", "{v:#}");
    // the module P(i@1)/x ⊕ P(i@1 i@2), its head, and its resolution
    let v = call(
        "module.head",
        json!({ "context": CYC, "summands": [{ "object": "i@1" }, { "object": "i@1 i@2" }] }),
    );
    assert_eq!(v["dimension"], 5, "{v:#}");
    let m = v["module"].clone();
    let v = call("module.check", json!({ "context": CYC, "module": m }));
    assert_eq!(v["holds"], true, "{v:#}");
    let mut r =
        call("resolution.start", json!({ "context": CYC, "module": m }))["resolution"].clone();
    for _ in 0..2 {
        r = call(
            "resolution.extend",
            json!({ "context": CYC, "resolution": r }),
        )["resolution"]
            .clone();
    }
    let v = call(
        "resolution.check",
        json!({ "context": CYC, "resolution": r }),
    );
    assert_eq!(v["holds"], true, "{v:#}");
    assert_eq!(v["uncovered_dimension"], 1, "{v:#}");
}

fn error(op: &str, req: Value) -> String {
    let v = ops::call(op, &req);
    assert_eq!(v["status"], "error", "{op}: {v:#}");
    v["message"].as_str().unwrap().to_string()
}

/// Every object of the complex formats, and every request, rejects a key it does not know, and
/// names the key and the nearest known one.
#[test]
fn misspelled_keys_are_rejected() {
    let mut c = two_term();
    c["diferentials"] = c["differentials"].take();
    c.as_object_mut().unwrap().remove("differentials");
    let e = error("complex.check", json!({ "context": CYC, "complex": c }));
    assert!(
        e.starts_with("a complex has no key 'diferentials' (did you mean 'differentials'?)"),
        "{e}"
    );
    let mut c = two_term();
    c["terms"][0]["summand"] = json!([]);
    let e = error("complex.check", json!({ "context": CYC, "complex": c }));
    assert!(
        e.contains("a term has no key 'summand' (did you mean 'summands'?)"),
        "{e}"
    );
    let mut c = two_term();
    c["differentials"][0]["form"] = json!(-1);
    let e = error("complex.check", json!({ "context": CYC, "complex": c }));
    assert!(
        e.contains("a differential has no key 'form' (did you mean 'from'?)"),
        "{e}"
    );
    let mut c = two_term();
    c["terms"][0]["summands"][0]["shfit"] = json!(2);
    let e = error("complex.check", json!({ "context": CYC, "complex": c }));
    assert!(
        e.contains("summand 1 has no key 'shfit' (did you mean 'shift'?)"),
        "{e}"
    );
    // a key with nothing close is named without a suggestion
    let mut c = two_term();
    c["colour"] = json!("red");
    let e = error("complex.check", json!({ "context": CYC, "complex": c }));
    assert!(e.contains("no key 'colour';"), "{e}");
    assert!(!e.contains("did you mean"), "{e}");
    // maps, modules, resolutions, and unknowns
    let e = error(
        "chainmap.check",
        json!({ "context": CYC, "source": two_term(), "target": two_term(),
                "map": [{ "at": 0, "matrix": [["e(i@1)"]], "matrx": [] }] }),
    );
    assert!(
        e.contains("a component of 'map' has no key 'matrx' (did you mean 'matrix'?)"),
        "{e}"
    );
    let e = error(
        "module.check",
        json!({ "context": CYC, "module": { "generators": [{ "object": "i@1" }], "relation": [] } }),
    );
    assert!(
        e.contains("a module has no key 'relation' (did you mean 'relations'?)"),
        "{e}"
    );
    let mut r = call(
        "resolution.start",
        json!({ "context": CYC, "module": { "head": "i@1" } }),
    )["resolution"]
        .clone();
    r["resolves"]["augmentaton"] = json!([["e(i@1)"]]);
    let e = error(
        "resolution.check",
        json!({ "context": CYC, "resolution": r }),
    );
    assert!(
        e.contains("'resolves' has no key 'augmentaton' (did you mean 'augmentation'?)"),
        "{e}"
    );
    let e = error(
        "complex.deform",
        json!({ "context": format!("{FREE} truncate=h^2"), "complex": two_term(),
                "unknowns": [{ "map": "differential", "from": -1, "rows": 0 }] }),
    );
    assert!(
        e.contains("unknown 0 has no key 'rows' (did you mean 'row'?)"),
        "{e}"
    );
    // requests: the keys are those of the operation's schema
    let e = error(
        "complex.homology",
        json!({ "context": CYC, "complex": two_term(), "at": 0, "max_degre": 4 }),
    );
    assert!(
        e.starts_with(
            "a complex.homology request has no key 'max_degre' (did you mean 'max_degree'?)"
        ),
        "{e}"
    );
    for op in diagcalc_core::ops_complex::OPS {
        let keys = diagcalc_core::ops_complex::request_keys(op);
        assert!(keys.iter().any(|k| k == "context"), "{op}: {keys:?}");
        let e = error(op, json!({ "context": CYC, "bogus_key": 1 }));
        assert!(e.contains("has no key 'bogus_key'"), "{op}: {e}");
    }
}

/// `basis: n` returns up to `n` homogeneous solutions, with a note exactly when that is fewer
/// than the freedom; written entries that vanish in the context are named in a note.
#[test]
fn solution_bases_and_vanishing_entries() {
    // End(P ⊕ P) in degree 0 is 2 × 2 scalar matrices: freedom 4
    let c =
        json!({ "terms": [{ "at": 0, "summands": [{ "object": "i@1" }, { "object": "i@1" }] }] });
    let solve = |basis: Value| {
        call(
            "chainmap.solve",
            json!({ "context": CYC, "source": c, "target": c, "basis": basis }),
        )
    };
    let notes = |v: &Value| -> Vec<String> {
        v["provenance"]["notes"]
            .as_array()
            .map_or(&[][..], Vec::as_slice)
            .iter()
            .map(|n| n.as_str().unwrap().to_string())
            .collect()
    };
    let v = solve(json!(2));
    assert_eq!(v["freedom"], 4, "{v:#}");
    assert_eq!(v["homogeneous_basis"].as_array().unwrap().len(), 2, "{v:#}");
    assert!(
        notes(&v)
            .iter()
            .any(|n| n.contains("'homogeneous_basis' lists 2 of the 4 basis vectors")),
        "{v:#}"
    );
    let v = solve(json!(4));
    assert_eq!(v["homogeneous_basis"].as_array().unwrap().len(), 4, "{v:#}");
    assert!(
        !notes(&v).iter().any(|n| n.contains("homogeneous_basis")),
        "{v:#}"
    );
    let v = solve(Value::Null);
    assert!(v.get("homogeneous_basis").is_none(), "{v:#}");
    assert!(
        !notes(&v).iter().any(|n| n.contains("homogeneous_basis")),
        "{v:#}"
    );
    let e = error(
        "chainmap.solve",
        json!({ "context": CYC, "source": c, "target": c, "basis": true }),
    );
    assert!(e.contains("'basis' is a nonnegative integer"), "{e}");

    // x_1 e(i@-1) vanishes in the cyclotomic quotient: echoed as null, with a note
    let mut c = two_term();
    c["differentials"][0]["matrix"] = json!([["x_1 e(i@-1)"]]);
    let v = call("complex.check", json!({ "context": CYC, "complex": c }));
    assert_eq!(
        v["complex"]["differentials"][0]["matrix"],
        json!([[null]]),
        "{v:#}"
    );
    assert!(
        notes(&v).contains(
            &"the differential from -1, entry (1, 1): 'x_1 e(i@-1)' is zero in this context, \
              so the entry is zero (shown as null)"
                .to_string()
        ),
        "{v:#}"
    );
    // a nonzero entry, and an entry written as zero, get no such note
    for entry in [json!("x_1 e(i@1)"), json!(null), json!("0")] {
        let mut c = two_term();
        c["differentials"][0]["matrix"] = json!([[entry]]);
        let v = call("complex.check", json!({ "context": CYC, "complex": c }));
        assert!(
            !notes(&v)
                .iter()
                .any(|n| n.contains("is zero in this context")),
            "{v:#}"
        );
    }
    // in a module, the entry is located in the presentation matrix
    let v = call(
        "module.check",
        json!({ "context": CYC, "module": {
            "generators": [{ "object": "i@1" }],
            "relations": [{ "object": "i@1", "shift": 2 }],
            "matrix": [["x_1 e(i@-1)"]] } }),
    );
    assert!(
        notes(&v)
            .iter()
            .any(|n| n.starts_with("the presentation matrix, entry (1, 1): 'x_1 e(i@-1)' is zero")),
        "{v:#}"
    );
}

/// The provenance notes of a result that say generators were dropped as redundant.
fn dropped_notes(v: &Value) -> Vec<String> {
    v["provenance"]["notes"]
        .as_array()
        .map_or(&[][..], Vec::as_slice)
        .iter()
        .filter_map(Value::as_str)
        .filter(|n| n.contains("was dropped") || n.contains("were dropped"))
        .map(String::from)
        .collect()
}

#[test]
fn generators_are_irredundant_with_a_scalar_of_negative_degree() {
    // With `h` of degree −2 and `h² = 0`, the degree sweep finds `h²` (degree −4) before `h`
    // (degree −2), which then produces it: the radical is generated by `h` and `x` alone, and
    // the resolution of the simple module has ranks 1, 2, 3, 4 (not 1, 3, 7, 15).
    let ctx = "klrw type=A1 ring=QQ red=\"2i@0\" quotient=cyclotomic truncate=h^2 hdeg=-2";
    let v = call("module.head", json!({ "context": ctx, "object": "i@1" }));
    assert_eq!(
        v["module"]["relations"],
        json!([{ "object": "i@1", "shift": -2 }, { "object": "i@1", "shift": 2 }]),
        "{v:#}"
    );
    assert_eq!(
        v["module"]["matrix"],
        json!([["h e(i@1)", "x_1 e(i@1)"]]),
        "{v:#}"
    );
    let d = dropped_notes(&v);
    assert_eq!(d.len(), 1, "{v:#}");
    assert!(d[0].starts_with("1 generator found earlier"), "{d:?}");
    assert!(d[0].contains("of the radical"), "{d:?}");
    // the same module with `x` alone: nothing to drop, no note
    let v = call("module.head", json!({ "context": CYC, "object": "i@1" }));
    assert!(dropped_notes(&v).is_empty(), "{v:#}");

    let h = call("module.head", json!({ "context": ctx, "object": "i@1" }));
    let v = call(
        "resolution.start",
        json!({ "context": ctx, "module": h["module"] }),
    );
    let mut r = v["resolution"].clone();
    let mut dropped = Vec::new();
    for n in 1..=3usize {
        let v = call(
            "resolution.extend",
            json!({ "context": ctx, "resolution": r }),
        );
        assert_eq!(
            v["added"]["summands"].as_array().unwrap().len(),
            n + 1,
            "{v:#}"
        );
        assert_eq!(v["check"]["holds"], true, "{v:#}");
        dropped.push(dropped_notes(&v));
        r = v["resolution"].clone();
    }
    // one, one, then two generators dropped
    assert!(
        dropped[0][0].starts_with("1 generator found"),
        "{dropped:?}"
    );
    assert!(
        dropped[1][0].starts_with("1 generator found"),
        "{dropped:?}"
    );
    assert!(
        dropped[2][0].starts_with("2 generators found"),
        "{dropped:?}"
    );
    assert!(dropped[2][0].contains("lie in") && dropped[2][0].contains("were dropped"));

    // complex.kernel at the far end of the resolution finds the same next term
    let v = call(
        "complex.kernel",
        json!({ "context": ctx, "complex": r, "at": -3 }),
    );
    assert_eq!(v["summands"].as_array().unwrap().len(), 5, "{v:#}");
    let d = dropped_notes(&v);
    assert_eq!(d.len(), 1, "{v:#}");
    assert!(d[0].contains("of the kernel"), "{d:?}");

    // without h, the worked example: one summand per step and no dropped note
    let v = call(
        "resolution.start",
        json!({ "context": CYC, "module": simple() }),
    );
    let v = call(
        "resolution.extend",
        json!({ "context": CYC, "resolution": v["resolution"] }),
    );
    assert_eq!(v["added"]["summands"].as_array().unwrap().len(), 1, "{v:#}");
    assert!(dropped_notes(&v).is_empty(), "{v:#}");
    let v = call(
        "complex.kernel",
        json!({ "context": CYC, "complex": v["resolution"], "at": -1 }),
    );
    assert!(dropped_notes(&v).is_empty(), "{v:#}");
}

#[test]
fn freedom_is_reported_when_no_solution_exists() {
    // `freedom` is the dimension of the homogeneous solutions, which exist whether or not the
    // inhomogeneous system has a solution; it once read 0 whenever there was none
    let c = json!({
        "terms": [p(-1, 2), p(0, 0)],
        "differentials": [{ "from": -1, "matrix": [["x_1 e(i@1)"]] }],
    });
    // P⟨2⟩ --0--> P
    let z = json!({
        "terms": [p(-1, 2), p(0, 0)],
        "differentials": [{ "from": -1, "matrix": [[null]] }],
    });
    // F(0) = 1 from z to z extends (F(−1) = λ), freedom 1
    let ok = call(
        "chainmap.solve",
        json!({ "context": CYC, "source": z, "target": z,
            "fixed": [{ "at": 0, "matrix": [["e(i@1)"]] }] }),
    );
    assert_eq!(ok["exists"], true, "{ok:#}");
    assert_eq!(ok["freedom"], 1, "{ok:#}");
    // F(0) = 1 from c to z would need 0 · F(−1) = 1 · x: no solution, F(−1) still free
    let none = call(
        "chainmap.solve",
        json!({ "context": CYC, "source": c, "target": z,
            "fixed": [{ "at": 0, "matrix": [["e(i@1)"]] }] }),
    );
    assert_eq!(none["exists"], false, "{none:#}");
    assert_eq!(none["freedom"], 1, "{none:#}");
    // on P --0--> P, the identity is not null-homotopic, and every h(0) = λ has d h + h d = 0
    let d = json!({
        "terms": [p(-1, 0), p(0, 0)],
        "differentials": [{ "from": -1, "matrix": [[null]] }],
    });
    let one = json!([
        { "at": -1, "matrix": [["e(i@1)"]] },
        { "at": 0, "matrix": [["e(i@1)"]] },
    ]);
    let v = call(
        "homotopy.find",
        json!({ "context": CYC, "source": d, "target": d, "f": one }),
    );
    assert_eq!(v["exists"], false, "{v:#}");
    assert_eq!(v["freedom"], 1, "{v:#}");
    let v = call(
        "homotopy.find",
        json!({ "context": CYC, "source": d, "target": d, "f": one, "g": one }),
    );
    assert_eq!(v["exists"], true, "{v:#}");
    assert_eq!(v["freedom"], 1, "{v:#}");
}

/// The provenance notes of a result.
fn all_notes(v: &Value) -> Vec<String> {
    v["provenance"]["notes"]
        .as_array()
        .map_or(&[][..], Vec::as_slice)
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect()
}

fn zero_notes(v: &Value) -> Vec<String> {
    all_notes(v)
        .into_iter()
        .filter(|n| n.contains("is zero in this context"))
        .collect()
}

/// `basis: n` lists homogeneous solutions also when no solution exists.
#[test]
fn homogeneous_basis_without_a_solution() {
    // X = Z as modules, P⟨2⟩ → P with differential x on X and 0 on Z; F₀ = 1 does not extend
    // (d_Z F₋₁ = 0 ≠ x = F₀ d_X), but F₋₁ alone, a scalar, is a chain map vanishing at 0
    let x = two_term();
    let mut z = two_term();
    z["differentials"][0]["matrix"] = json!([[null]]);
    let v = call(
        "chainmap.solve",
        json!({ "context": CYC, "source": x, "target": z,
                "fixed": [{ "at": 0, "matrix": [["e(i@1)"]] }], "basis": 3 }),
    );
    assert_eq!(v["exists"], false, "{v:#}");
    assert_eq!(v["freedom"], 1, "{v:#}");
    assert_eq!(
        v["homogeneous_basis"],
        json!([[{ "at": -1, "matrix": [["e(i@1)"]] }]]),
        "{v:#}"
    );
    // the basis lists every vector, so there is no note saying it does not
    assert!(
        !all_notes(&v)
            .iter()
            .any(|n| n.contains("homogeneous_basis")),
        "{v:#}"
    );
}

/// `resolves` is checked by every operation that reads a complex, and complex.check echoes it.
#[test]
fn resolves_is_checked_everywhere() {
    let mut garbage = two_term();
    garbage["resolves"] = json!("junk");
    let mut misspelled = two_term();
    misspelled["resolves"] = json!({ "modul": simple() });
    let reqs = |c: &Value| {
        vec![
            ("complex.check", json!({ "complex": c })),
            ("complex.homology", json!({ "complex": c })),
            ("complex.kernel", json!({ "complex": c, "at": -1 })),
            ("complex.minimal", json!({ "complex": c })),
            ("complex.euler", json!({ "complex": c })),
            ("complex.render", json!({ "complex": c })),
            ("complex.shift", json!({ "complex": c, "by": 1 })),
            ("complex.totalize", json!({ "complex": c })),
            ("complex.hom", json!({ "source": c, "target": two_term() })),
            (
                "chainmap.check",
                json!({ "source": c, "target": c, "map": [] }),
            ),
            ("chainmap.solve", json!({ "source": c, "target": c })),
            (
                "homotopy.find",
                json!({ "source": c, "target": c, "f": [] }),
            ),
            (
                "complex.cone",
                json!({ "source": c, "target": c, "map": [] }),
            ),
        ]
    };
    for c in [&garbage, &misspelled] {
        for (op, mut req) in reqs(c) {
            req["context"] = json!(CYC);
            let e = error(op, req);
            assert!(
                e.contains("'resolves' is {") || e.contains("did you mean 'module'"),
                "{op}: {e}"
            );
        }
    }
    let r = call(
        "resolution.start",
        json!({ "context": CYC, "module": simple() }),
    )["resolution"]
        .clone();
    let v = call("complex.check", json!({ "context": CYC, "complex": r }));
    assert_eq!(v["complex"]["resolves"], r["resolves"], "{v:#}");
    let v = call(
        "complex.check",
        json!({ "context": CYC, "complex": two_term() }),
    );
    assert!(v["complex"].get("resolves").is_none(), "{v:#}");
}

/// module.head takes 'shift' only with 'object'.
#[test]
fn module_head_rejects_shift_with_summands() {
    let e = error(
        "module.head",
        json!({ "context": CYC, "summands": [{ "object": "i@1" }], "shift": 2 }),
    );
    assert!(e.contains("'shift' goes with 'object'"), "{e}");
    let v = call(
        "module.head",
        json!({ "context": CYC, "summands": [{ "object": "i@1", "shift": 2 }] }),
    );
    assert_eq!(v["module"]["generators"][0]["shift"], 2, "{v:#}");
}

/// Every operation names a written entry that is zero in the context: in differentials, in
/// maps, and in augmentations; and none when every entry is null, "0", or nonzero.
#[test]
fn zero_entries_are_noted_in_every_operation() {
    let mut c = two_term();
    c["differentials"][0]["matrix"] = json!([["x_1 e(i@1) - x_1 e(i@1)"]]);
    let want = "the differential from -1, entry (1, 1): 'x_1 e(i@1) - x_1 e(i@1)' is zero in this \
                context, so the entry is zero (shown as null)";
    for (op, mut req) in [
        ("complex.check", json!({ "complex": c })),
        ("complex.homology", json!({ "complex": c })),
        ("complex.kernel", json!({ "complex": c, "at": 0 })),
        ("complex.minimal", json!({ "complex": c })),
        ("complex.gauss", json!({ "complex": c })),
        ("complex.euler", json!({ "complex": c })),
        ("complex.render", json!({ "complex": c })),
        ("complex.hom", json!({ "source": c, "target": c })),
        (
            "complex.cone",
            json!({ "source": c, "target": c, "map": [] }),
        ),
        ("chainmap.solve", json!({ "source": c, "target": c })),
        (
            "homotopy.find",
            json!({ "source": c, "target": c, "f": [] }),
        ),
    ] {
        req["context"] = json!(CYC);
        let v = call(op, req);
        assert_eq!(zero_notes(&v), vec![want.to_string()], "{op}: {v:#}");
    }
    for (op, req) in [
        ("complex.homology", json!({ "complex": two_term() })),
        ("complex.gauss", json!({ "complex": two_term() })),
        (
            "chainmap.solve",
            json!({ "source": two_term(), "target": two_term() }),
        ),
    ] {
        let mut req = req;
        req["context"] = json!(CYC);
        let v = call(op, req);
        assert!(zero_notes(&v).is_empty(), "{op}: {v:#}");
    }
    // a component of a chain map
    let v = call(
        "chainmap.check",
        json!({ "context": CYC, "source": two_term(), "target": two_term(),
                "map": [{ "at": 0, "matrix": [["e(i@1) - e(i@1)"]] }] }),
    );
    assert_eq!(
        zero_notes(&v),
        vec![
            "the component of 'map' at 0, entry (1, 1): 'e(i@1) - e(i@1)' is zero in this \
             context, so the entry is zero (shown as null)"
                .to_string()
        ],
        "{v:#}"
    );
    let v = call(
        "chainmap.check",
        json!({ "context": CYC, "source": two_term(), "target": two_term(),
                "map": [{ "at": 0, "matrix": [["e(i@1)"]] }] }),
    );
    assert!(zero_notes(&v).is_empty(), "{v:#}");
    // an augmentation
    let mut r = call(
        "resolution.start",
        json!({ "context": CYC, "module": simple() }),
    )["resolution"]
        .clone();
    r["resolves"]["augmentation"] = json!([["x_1 e(i@-1)"]]);
    let v = call(
        "resolution.check",
        json!({ "context": CYC, "resolution": r }),
    );
    assert_eq!(
        zero_notes(&v),
        vec![
            "the augmentation, entry (1, 1): 'x_1 e(i@-1)' is zero in this context, so the \
             entry is zero (shown as null)"
                .to_string()
        ],
        "{v:#}"
    );
}

/// The empty loading prints as its idempotent `e()`, never as an empty string.
#[test]
fn the_empty_loading_is_printed() {
    // the unit object (no black strand) beside P(i@1)
    let c =
        json!({ "terms": [{ "at": 0, "summands": [{ "object": "e()" }, { "object": "i@1" }] }] });
    let v = call("complex.check", json!({ "context": CYC, "complex": c }));
    assert_eq!(
        v["complex"]["terms"][0]["summands"][0]["object"], "e()",
        "{v:#}"
    );
    let v = call("complex.homology", json!({ "context": CYC, "complex": c }));
    let first = all_notes(&v)[0].clone();
    assert!(first.contains("e()"), "{first}");
    assert!(!first.contains(": ,") && !first.contains(", ,"), "{first}");
    for pos in v["positions"].as_array().unwrap() {
        for piece in pos["pieces"].as_array().unwrap() {
            assert_ne!(piece["test"], "", "{v:#}");
        }
    }
}

/// Integers out of range are errors, not overflows, and a search wider than the cap is refused
/// rather than run.
#[test]
fn out_of_range_integers_and_wide_searches() {
    let mut c = two_term();
    c["terms"][0]["summands"][0]["shift"] = json!(i64::MAX);
    for op in ["complex.check", "complex.euler", "complex.homology"] {
        let e = error(op, json!({ "context": CYC, "complex": c }));
        assert!(e.contains("outside the supported range"), "{op}: {e}");
    }
    let e = error(
        "module.head",
        json!({ "context": CYC, "object": "i@1", "shift": i64::MIN }),
    );
    assert!(e.contains("outside the supported range"), "{e}");
    let mut c = two_term();
    c["terms"][0]["at"] = json!(i64::MAX);
    let e = error("complex.check", json!({ "context": CYC, "complex": c }));
    assert!(e.contains("outside the supported range"), "{e}");
    // without the quotient the Hom spaces are unbounded: the cap is reported, not searched
    for (op, req) in [
        ("complex.homology", json!({ "complex": two_term() })),
        ("complex.kernel", json!({ "complex": two_term(), "at": -1 })),
        ("hom.basis", json!({ "source": "i@1", "target": "i@1" })),
    ] {
        let mut req = req;
        req["context"] = json!(FREE);
        req["max_degree"] = json!(i64::MAX);
        let e = error(op, req.clone());
        assert!(e.contains("outside the supported range"), "{op}: {e}");
        req["max_degree"] = json!(1 << 30);
        let e = error(op, req.clone());
        assert!(e.contains("at most 1000 are searched at once"), "{op}: {e}");
        req["max_degree"] = json!(8);
        call(op, req);
    }
    // summands whose shifts lie far apart, even with bounded Hom spaces
    let far = json!({ "terms": [{ "at": 0, "summands": [
        { "object": "i@1", "shift": 0 }, { "object": "i@1", "shift": 100000 }] }] });
    let e = error(
        "complex.homology",
        json!({ "context": CYC, "complex": far }),
    );
    assert!(e.contains("at most 1000 are searched at once"), "{e}");
}

/// complex.deform refuses an entry that complex.check refuses: here not homogeneous.
#[test]
fn deform_refuses_inhomogeneous_entries() {
    let ctx = format!("{CYC} truncate=h^2 hdeg=0");
    let mut c = two_term();
    c["differentials"][0]["matrix"] = json!([["x_1 e(i@1) + h e(i@1)"]]);
    let v = call("complex.check", json!({ "context": ctx, "complex": c }));
    assert_eq!(v["holds"], false, "{v:#}");
    let e = error("complex.deform", json!({ "context": ctx, "complex": c }));
    assert!(e.contains("not a map of the right degree"), "{e}");
    call(
        "complex.deform",
        json!({ "context": ctx, "complex": two_term() }),
    );
}
