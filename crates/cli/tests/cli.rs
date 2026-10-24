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
fn every_computed_result_says_how_it_was_obtained() {
    // an odd BE2 normal form rests on the Nondegeneracy Conjecture, and says so
    let (ok, out) = diagcalc(&["run", "examples/be2-a1-odd.dcw"]);
    assert!(ok, "{out}");
    assert!(
        out.contains("wt1:")
            && out.contains("certificate conditional: ")
            && out.contains("Nondegeneracy Conjecture"),
        "{out}"
    );
    // KL's normal form rests on the literature
    let (ok, out) = diagcalc(&["run", "examples/kl-a1-bubbles.dcw"]);
    assert!(
        ok && out.contains("certificate literature: Khovanov-Lauda"),
        "{out}"
    );
    // a verdict the relations alone prove says it needs no certificate; one that rests on the
    // conjecture does not claim that
    let (_, out) = diagcalc(&[
        "check",
        "-c",
        "be2 type=A1 odd=i mu=-1",
        "zero",
        "psi_1 psi_1 e(ii)",
    ]);
    assert!(
        out.starts_with("holds") && out.contains("proven by the computation alone"),
        "{out}"
    );
    let (_, out) = diagcalc(&["check", "-c", "be2 type=A1 odd=i mu=-1", "zero", "x_1 e(i)"]);
    assert!(
        out.starts_with("fails")
            && out.contains("certificate conditional")
            && !out.contains("alone"),
        "{out}"
    );
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
