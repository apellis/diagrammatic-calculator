//! The Lean export of rewrite chains: the text is produced; building it needs Lean and
//! categorification-lean (see `lean/chains/`).

use diagcalc_core::builtin;
use diagcalc_core::context::{Context, ContextSpec};
use diagcalc_core::lean::{self, Item};
use diagcalc_core::rep;
use diagcalc_core::rewrite;
use diagcalc_core::ring::{Integers, Ring};

fn ctx<R: Ring>(spec: &str, base: R) -> Context<R> {
    let spec = ContextSpec::parse(spec).unwrap();
    let pres = builtin::presentation(&spec.presentation).unwrap();
    Context::new(pres, spec, base).unwrap()
}

#[test]
fn export_cross_dots_cross() {
    let c = ctx("klr type=A2 nu=i+j", Integers);
    let rules = rewrite::rules(&c).unwrap();
    let layout = rep::basis_layout(&c, &c.pres.representations[0]).unwrap();
    let t = c.parse_term("psi_1 x_2 x_2 psi_1 e(ij)").unwrap();
    let chain = rewrite::straighten(&c, &rules, &layout, &t, 1000).unwrap();
    let text = lean::export_file(
        &c,
        &[Item {
            name: "diagcalc_test".into(),
            doc: String::new(),
            chains: vec![&chain],
        }],
        "",
    )
    .unwrap();
    if let Ok(p) = std::env::var("DIAGCALC_LEAN_OUT") {
        std::fs::write(p, &text).unwrap();
    }
    assert!(text.contains("dg_stepL_at"));
}

/// A batch of random straightening chains in several Cartan types, for building in Lean.
#[test]
fn export_random_batch() {
    let mut items_text = Vec::new();
    let specs = [
        ("klr type=A2 nu=2i+j", 6),
        ("klr type=B2 nu=i+2j", 6),
        ("klr type=G2 nu=i+j", 5),
        ("klr type=A3 nu=i+j+k", 6),
        ("klr type=A3 nu=i+2k", 5),
    ];
    let mut n = 0;
    for (spec, len) in specs {
        let c = ctx(spec, Integers);
        let rules = rewrite::rules(&c).unwrap();
        let layout = rep::basis_layout(&c, &c.pres.representations[0]).unwrap();
        let mut x: u64 = 11;
        let mut next = |m: usize| {
            x = x
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((x >> 33) as usize) % m
        };
        let mut chains = Vec::new();
        for _ in 0..40 {
            let obj = c.objects[next(c.objects.len())].clone();
            let nn = obj.len();
            let mut parts = Vec::new();
            for _ in 0..len {
                if next(3) > 0 {
                    parts.push(format!("psi_{}", next(nn - 1) + 1));
                } else {
                    parts.push(format!("x_{}", next(nn) + 1));
                }
            }
            parts.push(format!("e({})", c.fmt_object(&obj)));
            let t = c.parse_term(&parts.join(" ")).unwrap();
            let chain = rewrite::straighten(&c, &rules, &layout, &t, 1000).unwrap();
            if chain.steps.is_empty() {
                continue;
            }
            // Chains whose relations have no Lean lemma in this case are refused, not exported.
            let probe = Item {
                name: "probe".into(),
                doc: String::new(),
                chains: vec![&chain],
            };
            if let Err(e) = lean::export_file(&c, &[probe], "") {
                assert!(e.message.contains("needs (α_"), "{e:?}");
                continue;
            }
            chains.push((parts.join(" "), chain));
            if chains.len() == 6 {
                break;
            }
        }
        let items: Vec<Item<Integers>> = chains
            .iter()
            .map(|(s, ch)| {
                n += 1;
                Item {
                    name: format!("diagcalc_batch_{n}"),
                    doc: format!(
                        "`{s}` in `{spec}`, straightened in {} steps.",
                        ch.steps.len()
                    ),
                    chains: vec![ch],
                }
            })
            .collect();
        items_text.push(lean::export_file(&c, &items, "").unwrap());
    }
    if let Ok(dir) = std::env::var("DIAGCALC_LEAN_DIR") {
        for (i, t) in items_text.iter().enumerate() {
            std::fs::write(format!("{dir}/Batch{i}.lean"), t).unwrap();
        }
    }
}

/// The generated files under `lean/chains/` are what `diagcalc lean-chains` writes now; CI builds
/// them in Lean (`.github/workflows/lean-chains.yml`).
#[test]
fn odd_chain_files_are_current() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../lean/odd-chains");
    let text = std::fs::read_to_string(dir.join("chains.toml")).unwrap();
    for (rel, want) in lean::export_batch(&text, "DiagcalcChains").unwrap() {
        let got = std::fs::read_to_string(dir.join(&rel)).unwrap_or_default();
        assert!(
            got == want,
            "lean/odd-chains/{rel} is out of date: run \
             `diagcalc lean-chains lean/odd-chains/chains.toml`"
        );
    }
}

#[test]
fn chain_files_are_current() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../lean/chains");
    let text = std::fs::read_to_string(dir.join("chains.toml")).unwrap();
    for (rel, want) in lean::export_batch(&text, "DiagcalcChains").unwrap() {
        let got = std::fs::read_to_string(dir.join(&rel)).unwrap_or_default();
        assert!(
            got == want,
            "lean/chains/{rel} is out of date: run `diagcalc lean-chains lean/chains/chains.toml`"
        );
    }
}
