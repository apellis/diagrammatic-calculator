//! Weighted KLR workspaces run end to end through the operation API.

use diagcalc_core::ops;

#[test]
fn ghost_workspace_runs() {
    let src = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/klrw-a2-ghosts.dcw"
    ))
    .unwrap();
    let v = ops::run_workspace_with(&src, None, Some(8), &[]);
    assert_eq!(v["status"], "ok", "{v}");
    let cells = v["cells"].as_array().unwrap();
    for c in cells {
        assert_eq!(c["status"], "ok", "{c}");
    }
    assert_eq!(cells[1]["normal_form"], "0", "{}", cells[1]);
    assert_eq!(cells[2]["holds"], true, "{}", cells[2]);
    assert_eq!(cells[3]["holds"], true, "{}", cells[3]);
    assert_eq!(
        cells[4]["normal_form"], "b(2 1; j@0 i@2) x_2 e(i@0 j@2)",
        "{}",
        cells[4]
    );
    let svg = cells[4]["summands"][0]["svg"].as_str().unwrap();
    assert!(svg.contains("class=\"ghost"), "{svg}");
}

#[test]
fn klrw_check_and_render() {
    let req = ops::Request {
        context: "klrw type=A1".into(),
        ..Default::default()
    };
    let v = ops::check(
        &req,
        "equal",
        &[
            "psi_1 x_1 e(i@0 i@1) - x_2 psi_1 e(i@0 i@1)".into(),
            "e(i@0 i@1)".into(),
        ],
    );
    assert_eq!(v["status"], "ok", "{v}");
    assert_eq!(v["holds"], true, "{v}");
    let r = ops::render(&req, "psi_1 e(i@0 i@1)", "svg");
    assert_eq!(r["status"], "ok", "{r}");
}

#[test]
fn klrw_describe() {
    let v = ops::describe_context("klrw type=A2 theta_ij=1/2", None);
    assert_eq!(v["status"], "ok", "{v}");
    assert_eq!(v["edges"][0]["tail"], "i", "{v}");
    assert_eq!(v["edges"][0]["weight"], "1/2", "{v}");
}

#[test]
fn red_bigon_is_a_power_of_the_dot() {
    let req = ops::Request {
        context: "klrw type=A1 red=\"2i@0\"".into(),
        ..Default::default()
    };
    let v = ops::simplify(&req, "to(i@-1) to(i@1) e(i@-1)");
    assert_eq!(v["status"], "ok", "{v}");
    assert_eq!(v["normal_form"], "x_1^2 e(i@-1)", "{v}");
}

#[test]
fn solve_and_trace_in_klrw() {
    // in the cyclotomic quotient by 2i@0, the algebra on one strand right of the red strand is
    // k[x]/(x^2); its idempotents c1 e + c2 x e are e (and 0)
    let src = "diagcalc-workspace/1
context klrw type=A1 ring=QQ red=\"2i@0\" quotient=cyclotomic
def f := c1 e(i@1) + c2 x_1 e(i@1)  # id=d1
watch f f - f  # id=w2
solve f f - f = 0 for c1, c2  # id=s3
def a := x_1 e(i@1 i@2) + psi_1 e(i@1 i@2)  # id=d4
watch a a  # id=w5
";
    let v = ops::run_workspace(src, None);
    assert_eq!(v["status"], "ok", "{v}");
    let cells = v["cells"].as_array().unwrap();
    for c in cells {
        assert_eq!(c["status"], "ok", "{c}");
    }
    assert_eq!(
        cells[2]["points"],
        serde_json::json!([["0", "0"], ["1", "0"]]),
        "{}",
        cells[2]
    );
    let word = ops::run_workspace_with(src, None, Some(8), &[])["cells"][4]["summands"][0]["word"]
        .as_str()
        .unwrap()
        .to_string();
    let t = ops::trace_workspace(src, None, &[], "w5", &word);
    assert_eq!(t["status"], "ok", "{t}");
    assert!(!t["sources"].as_array().unwrap().is_empty(), "{t}");
}

#[test]
fn resolution_workspace_runs() {
    let src = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/klrw-a1-resolution.dcw"
    ))
    .unwrap();
    let v = ops::run_workspace_with(&src, None, Some(8), &[]);
    assert_eq!(v["status"], "ok", "{v}");
    let cells: Vec<_> = v["cells"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["kind"] == "module" || c["kind"] == "complex")
        .collect();
    assert_eq!(cells.len(), 2, "{v}");
    assert_eq!(cells[0]["holds"], true, "{}", cells[0]);
    assert_eq!(cells[0]["dimension"], 1, "{}", cells[0]);
    let r = &cells[1]["resolution"];
    assert_eq!(r["holds"], true, "{}", cells[1]);
    assert_eq!(r["resolved"], false, "{}", cells[1]);
    assert_eq!(r["next_at"], -2, "{}", cells[1]);
    // the module name was replaced by the module
    assert_eq!(
        cells[1]["complex"]["resolves"]["module"]["generators"][0]["object"], "i@1",
        "{}",
        cells[1]
    );
}

#[test]
fn head_module_is_the_presentation_by_rad() {
    let src = "diagcalc-workspace/1\ncontext klrw type=A1 ring=QQ red=2i@0 quotient=cyclotomic\n\n\
               module L := {\"head\": \"i@1\"}\n\
               module M := {\"head\": [{\"object\": \"i@1\", \"shift\": 3}]}\n\
               module N := {\"head\": \"i@1\", \"generators\": []}\n";
    let v = ops::run_workspace_with(src, None, Some(8), &[]);
    let cells: Vec<_> = v["cells"].as_array().unwrap().iter().collect();
    let l = &cells[0];
    assert_eq!(l["holds"], true, "{l}");
    assert_eq!(l["module"]["relations"][0]["shift"], 2, "{l}");
    assert_eq!(l["module"]["matrix"][0][0], "x_1 e(i@1)", "{l}");
    // what the web view draws: the strands of each object with the red one, and each entry
    assert_eq!(
        l["display"]["objects"]["i@1"],
        serde_json::json!([
            { "label": "2i", "at": "0", "red": true },
            { "label": "i", "at": "1", "red": false }
        ]),
        "{l}"
    );
    let e = &l["display"]["entries"]["x_1 e(i@1)"];
    assert_eq!(e["more"], 0, "{l}");
    assert!(
        e["summands"][0]["svg"].as_str().unwrap().contains("<svg"),
        "{l}"
    );
    assert_eq!(
        cells[1]["module"]["generators"][0]["shift"], 3,
        "{}",
        cells[1]
    );
    assert_eq!(
        cells[1]["module"]["relations"][0]["shift"], 5,
        "{}",
        cells[1]
    );
    assert_eq!(cells[2]["status"], "error", "{}", cells[2]);
}

#[test]
fn resolution_cell_needs_its_module() {
    let src = "diagcalc-workspace/1\ncontext klrw type=A1 ring=QQ\n\ncomplex R := \
               {\"terms\": [{\"at\": 0, \"summands\": [{\"object\": \"i@0\", \"shift\": 0}]}], \
               \"differentials\": [], \"resolves\": {\"module\": \"L\"}}\n";
    let v = ops::run_workspace_with(src, None, Some(8), &[]);
    let c = v["cells"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["kind"] == "complex")
        .unwrap()
        .clone();
    assert_eq!(c["status"], "error", "{c}");
    assert!(
        c["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("no module cell")
            || c.to_string().contains("no module cell"),
        "{c}"
    );
}
