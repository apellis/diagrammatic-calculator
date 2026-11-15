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
