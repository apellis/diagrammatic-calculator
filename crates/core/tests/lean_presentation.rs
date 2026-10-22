//! The KLR presentation file against the Lean definition of the KLR relations.
//!
//! `lean/chains/klr-relations.txt` is printed from categorification-lean's
//! `KLR.Diagram.relation` by `lean/chains/PrintRelations.lean` (CI checks it is current). Every
//! printed relation, at every colouring its side condition allows, must be one of the
//! calculator's rules up to sign and must vanish in the polynomial representation; and every
//! relation of `presentations/klr.pres.toml` must arise this way.

use diagcalc_core::builtin;
use diagcalc_core::context::{Context, ContextSpec};
use diagcalc_core::rep;
use diagcalc_core::rewrite;
use diagcalc_core::ring::{Integers, Ring};
use std::collections::BTreeSet;

struct LeanRelation {
    name: String,
    vars: Vec<String>,
    condition: String,
    term: String,
}

fn lean_relations() -> Vec<LeanRelation> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../lean/chains/klr-relations.txt");
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let f: Vec<&str> = l.split(';').map(str::trim).collect();
            assert_eq!(f.len(), 4, "{l}");
            LeanRelation {
                name: f[0].into(),
                vars: f[1].split_whitespace().map(String::from).collect(),
                condition: f[2].into(),
                term: f[3].into(),
            }
        })
        .collect()
}

/// Evaluates a side condition printed by Lean: `=`, `≠`, `∧`, `∨`, `¬` and parentheses over
/// the colour variables.
fn holds(cond: &str, value: &dyn Fn(&str) -> char) -> bool {
    fn expr(t: &[String], i: &mut usize, v: &dyn Fn(&str) -> char) -> bool {
        let mut a = conj(t, i, v);
        while t.get(*i).map(String::as_str) == Some("∨") {
            *i += 1;
            a |= conj(t, i, v);
        }
        a
    }
    fn conj(t: &[String], i: &mut usize, v: &dyn Fn(&str) -> char) -> bool {
        let mut a = atom(t, i, v);
        while t.get(*i).map(String::as_str) == Some("∧") {
            *i += 1;
            a &= atom(t, i, v);
        }
        a
    }
    fn atom(t: &[String], i: &mut usize, v: &dyn Fn(&str) -> char) -> bool {
        match t[*i].as_str() {
            "¬" => {
                *i += 1;
                !atom(t, i, v)
            }
            "(" => {
                *i += 1;
                let a = expr(t, i, v);
                assert_eq!(t[*i], ")");
                *i += 1;
                a
            }
            x => {
                let (op, y) = (&t[*i + 1], &t[*i + 2]);
                *i += 3;
                match op.as_str() {
                    "=" => v(x) == v(y),
                    "≠" => v(x) != v(y),
                    o => panic!("unknown operator {o}"),
                }
            }
        }
    }
    if cond.is_empty() {
        return true;
    }
    let spaced = cond
        .replace('(', " ( ")
        .replace(')', " ) ")
        .replace('¬', " ¬ ");
    let tokens: Vec<String> = spaced.split_whitespace().map(String::from).collect();
    let mut i = 0;
    let r = expr(&tokens, &mut i, value);
    assert_eq!(i, tokens.len(), "{cond}");
    r
}

/// The term with colour variables replaced: inside `e(…)` and in the subscripts of `Q`, `Qbar`.
fn colour(term: &str, value: &dyn Fn(&str) -> char) -> String {
    let mut out = String::new();
    let mut rest = term;
    while !rest.is_empty() {
        if let Some(r) = rest.strip_prefix("e(") {
            let end = r.find(')').unwrap();
            out.push_str("e(");
            out.extend(r[..end].split_whitespace().map(value));
            out.push(')');
            rest = &r[end + 1..];
        } else if let Some((pre, r)) = ["Qbar_", "Q_"]
            .iter()
            .find_map(|p| rest.strip_prefix(p).map(|r| (*p, r)))
        {
            let end = r.find('(').unwrap();
            out.push_str(pre);
            out.extend(r[..end].chars().map(|c| value(&c.to_string())));
            rest = &r[end..];
        } else {
            let c = rest.chars().next().unwrap();
            out.push(c);
            rest = &rest[c.len_utf8()..];
        }
    }
    out
}

#[test]
fn lean_relations_match_the_presentation() {
    let lean = lean_relations();
    assert_eq!(
        lean.len(),
        8,
        "the Lean presentation has eight families of relations"
    );
    let mut covered: BTreeSet<String> = BTreeSet::new();
    let mut checked = 0;
    for (ty, colours) in [("A2", "ij"), ("B2", "ij"), ("G2", "ij"), ("A3", "ijk")] {
        let colours: Vec<char> = colours.chars().collect();
        for r in &lean {
            let n = r.vars.len();
            for code in 0..colours.len().pow(n as u32) {
                let pick: Vec<char> = (0..n)
                    .map(|j| colours[code / colours.len().pow(j as u32) % colours.len()])
                    .collect();
                let value = |v: &str| pick[r.vars.iter().position(|x| x == v).unwrap()];
                if !holds(&r.condition, &value) {
                    continue;
                }
                let text = colour(&r.term, &value);
                let mut nu = std::collections::BTreeMap::new();
                let obj = &text[text.rfind("e(").unwrap() + 2..text.len() - 1];
                for c in obj.chars() {
                    *nu.entry(c).or_insert(0) += 1;
                }
                let nu: Vec<String> = nu
                    .iter()
                    .map(|(c, m)| {
                        if *m == 1 {
                            c.to_string()
                        } else {
                            format!("{m}{c}")
                        }
                    })
                    .collect();
                let spec =
                    ContextSpec::parse(&format!("klr type={ty} nu={}", nu.join("+"))).unwrap();
                let ctx = Context::new(
                    builtin::presentation(&spec.presentation).unwrap(),
                    spec,
                    Integers,
                )
                .unwrap();
                let t = ctx
                    .parse_term(&text)
                    .unwrap_or_else(|e| panic!("{text}: {e}"));
                let ev = rep::evaluators(&ctx).unwrap().remove(0);
                assert!(
                    ev.normal_form(&t).unwrap().terms.is_empty(),
                    "{} in type {ty}: {text} is not zero",
                    r.name
                );
                let found = rewrite::rules(&ctx).unwrap().into_iter().find(|rule| {
                    let one = ctx.coeff.one();
                    let diff = ctx.add(
                        &ctx.monomial_term(&rule.pattern, one),
                        &ctx.neg(&rule.replacement),
                    );
                    ctx.add(&t, &diff).terms.is_empty()
                        || ctx.add(&t, &ctx.neg(&diff)).terms.is_empty()
                });
                let rule = found.unwrap_or_else(|| {
                    panic!(
                        "{} in type {ty}: {text} is no relation of the calculator",
                        r.name
                    )
                });
                covered.insert(rule.relation);
                checked += 1;
            }
        }
    }
    let ctx_pres = builtin::presentation("klr").unwrap();
    let all: BTreeSet<String> = ctx_pres.relations.iter().map(|r| r.name.clone()).collect();
    assert_eq!(covered, all, "relations of the file not found among Lean's");
    assert!(checked > 40, "only {checked} instances checked");
}
