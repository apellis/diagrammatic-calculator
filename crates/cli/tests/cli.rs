//! The command-line tool on the example workspaces and a few one-off commands.

use std::process::Command;

fn diagcalc(args: &[&str]) -> (bool, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_diagcalc"))
        .args(args)
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .output()
        .unwrap();
    (out.status.success(), String::from_utf8(out.stdout).unwrap())
}

#[test]
fn examples_run_and_are_canonical() {
    for entry in std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples")).unwrap()
    {
        let path = entry.unwrap().path();
        let p = path.to_str().unwrap();
        let (ok, out) = diagcalc(&["run", p]);
        assert!(
            ok && !out.contains("fails") && !out.contains("error"),
            "{p}:\n{out}"
        );
        let (ok, out) = diagcalc(&["fmt", p]);
        assert!(ok);
        assert_eq!(
            out,
            std::fs::read_to_string(&path).unwrap(),
            "{p} is not in canonical form"
        );
    }
}

#[test]
fn one_off_commands() {
    let (ok, out) = diagcalc(&["simplify", "-c", "nilhecke nu=2", "x_1 psi_1"]);
    assert!(ok);
    assert!(out.starts_with("1 + psi_1 x_2\n"), "{out}");
    let (ok, out) = diagcalc(&[
        "check",
        "-c",
        "klr type=A2 nu=2i+j",
        "idempotent",
        "psi_1 e(iji)",
    ]);
    assert!(!ok && out.starts_with("fails"), "{out}");
    let (ok, out) = diagcalc(&[
        "draw",
        "-c",
        "klr type=A2 nu=i+j",
        "--format",
        "svg",
        "psi_1 e(ij)",
    ]);
    assert!(ok && out.starts_with("<svg"), "{out}");
    let (ok, out) = diagcalc(&["check-presentation", "presentations/odd-nilhecke.pres.toml"]);
    assert!(ok && out.contains("0 failed"), "{out}");
    let (ok, out) = diagcalc(&["simplify", "--json", "-c", "nilhecke nu=2", "psi_1 psi_1"]);
    assert!(ok && out.contains("\"normal_form\": \"0\""), "{out}");
}
