//! The conformance checker: does a presentation, in a context, mean what it says?
//!
//! For every instance of every relation schema it checks that both sides elaborate, that all
//! words on both sides share one source, target, degree and parity, and that the relation holds
//! in every representation the file declares. It checks the parameter hypotheses it recognizes
//! and reports the others as stated but unchecked, and flags relation schemas whose side
//! condition no colour binding satisfies.

use crate::builtin;
use crate::context::{Context, ContextSpec};
use crate::elab::{CTerm, Scope};
use crate::error::Result;
use crate::pres::{ColourSpec, Presentation, Relation};
use crate::rep;
use crate::ring::{Rationals, Ring};
use crate::text;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Pass,
    Fail,
    /// Stated in the file but not something the checker can verify.
    Unchecked,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub check: String,
    pub status: Status,
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Report {
    pub presentation: String,
    pub context: String,
    pub findings: Vec<Finding>,
}

impl Report {
    pub fn ok(&self) -> bool {
        self.findings.iter().all(|f| f.status != Status::Fail)
    }
    pub fn failures(&self) -> impl Iterator<Item = &Finding> {
        self.findings.iter().filter(|f| f.status == Status::Fail)
    }
}

fn finding(check: String, ok: bool, detail: impl Into<String>) -> Finding {
    Finding {
        check,
        status: if ok { Status::Pass } else { Status::Fail },
        detail: detail.into(),
    }
}

/// One instance of a relation schema: its colour variables bound, and its integer variables.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Instance {
    pub colours: Vec<usize>,
    pub ints: Vec<i64>,
}

/// All instances of a relation schema: colour bindings that satisfy its side condition, with
/// every value of its integer variables in their ranges (evaluated at the context's `μ`).
pub fn instances<R: Ring>(ctx: &Context<R>, rel: &Relation) -> Result<Vec<Instance>> {
    let k = ctx.colours.len();
    let m = rel.vars.len();
    let mut out = Vec::new();
    let total = k.pow(m as u32);
    for code in 0..total {
        let mut b = Vec::with_capacity(m);
        let mut c = code;
        for _ in 0..m {
            b.push(c % k);
            c /= k;
        }
        b.reverse();
        if !rel.when.eval(&b, ctx)? {
            continue;
        }
        let scope = colour_scope::<R>(rel, &b);
        let mut ranges = Vec::new();
        for (_, lo, hi) in &rel.ints {
            ranges.push((ctx.eval_int(lo, &scope)?, ctx.eval_int(hi, &scope)?));
        }
        let mut vals: Vec<Vec<i64>> = vec![vec![]];
        for (lo, hi) in ranges {
            vals = vals
                .into_iter()
                .flat_map(|v| {
                    (lo..=hi).map(move |x| {
                        let mut v = v.clone();
                        v.push(x);
                        v
                    })
                })
                .collect();
        }
        for ints in vals {
            out.push(Instance {
                colours: b.clone(),
                ints,
            });
        }
    }
    Ok(out)
}

fn colour_scope<'a, R: Ring>(rel: &Relation, b: &[usize]) -> Scope<'a, R> {
    Scope {
        colour_vars: rel
            .vars
            .iter()
            .cloned()
            .zip(b.iter().map(|&c| c as u16))
            .collect(),
        ..Scope::default()
    }
}

/// Both sides of one relation instance.
pub fn instance_terms<R: Ring>(
    ctx: &Context<R>,
    rel: &Relation,
    inst: &Instance,
) -> Result<(CTerm<R>, CTerm<R>)> {
    let mut scope = colour_scope::<R>(rel, &inst.colours);
    scope.ints = rel
        .ints
        .iter()
        .map(|(v, _, _)| v.clone())
        .zip(inst.ints.iter().copied())
        .collect();
    let mut sides =
        ctx.elaborate_together(&[text::parse(&rel.lhs)?, text::parse(&rel.rhs)?], &scope)?;
    let rhs = sides.pop().unwrap();
    let lhs = sides.pop().unwrap();
    Ok((lhs, rhs))
}

fn binding_text<R: Ring>(ctx: &Context<R>, rel: &Relation, b: &Instance) -> String {
    if b.colours.is_empty() && b.ints.is_empty() {
        return String::new();
    }
    let parts: Vec<String> = rel
        .vars
        .iter()
        .zip(&b.colours)
        .map(|(v, &c)| format!("{v}={}", ctx.colours[c].name))
        .chain(
            rel.ints
                .iter()
                .zip(&b.ints)
                .map(|((v, _, _), n)| format!("{v}={n}")),
        )
        .collect();
    format!(" [{}]", parts.join(", "))
}

/// Checks a presentation in one context. Relations are checked at the objects they name, not
/// only at those of the context's weight.
pub fn check<R: Ring>(ctx: &Context<R>) -> Report {
    let mut free = ctx.clone();
    free.objects.clear();
    free.weight = None;
    let ctx = &free;
    let mut findings = Vec::new();
    let evals = match rep::evaluators(ctx) {
        Ok(e) => e,
        Err(e) => {
            findings.push(finding("representations".into(), false, e.to_string()));
            vec![]
        }
    };
    for rel in &ctx.pres.relations {
        let bindings = match instances(ctx, rel) {
            Ok(b) => b,
            Err(e) => {
                findings.push(finding(
                    format!("relation {}", rel.name),
                    false,
                    e.to_string(),
                ));
                continue;
            }
        };
        if bindings.is_empty() {
            findings.push(Finding {
                check: format!("relation {}", rel.name),
                status: Status::Unchecked,
                detail: "no colour binding in this context satisfies the side condition".into(),
            });
        }
        for b in bindings {
            let name = format!("relation {}{}", rel.name, binding_text(ctx, rel, &b));
            let (lhs, rhs) = match instance_terms(ctx, rel, &b) {
                Ok(x) => x,
                Err(e) => {
                    findings.push(finding(name, false, e.to_string()));
                    continue;
                }
            };
            if lhs.is_zero() && rhs.is_zero() {
                findings.push(finding(name, false, "both sides elaborate to zero"));
                continue;
            }
            // typing: one source, target, degree and parity across both sides
            let words: Vec<_> = lhs.terms.iter().chain(&rhs.terms).map(|(w, _)| w).collect();
            let w0 = words[0];
            let key = |w: &crate::term::Word| {
                (
                    w.source.clone(),
                    ctx.target(w),
                    ctx.word_degree(w),
                    ctx.word_odd(w),
                )
            };
            let k0 = key(w0);
            match words.iter().find(|w| key(w) != k0) {
                Some(w) => findings.push(finding(
                    format!("{name}: typing"),
                    false,
                    format!(
                        "{} and {} differ in source, target, degree or parity",
                        ctx.fmt_word(w0),
                        ctx.fmt_word(w)
                    ),
                )),
                None => findings.push(finding(
                    format!("{name}: typing"),
                    true,
                    format!("degree {}", k0.2),
                )),
            }
            let diff = ctx.sub(&lhs, &rhs);
            for ev in &evals {
                let check = format!("{name}: holds in {}", ev.provenance().method);
                match ev.is_zero(&diff) {
                    Ok(true) => findings.push(finding(check, true, "")),
                    Ok(false) => {
                        let nf = ev
                            .normal_form(&diff)
                            .map(|t| ctx.fmt_term(&t))
                            .unwrap_or_default();
                        findings.push(finding(check, false, format!("lhs − rhs = {nf}")))
                    }
                    Err(e) => findings.push(finding(check, false, e.to_string())),
                }
            }
        }
    }
    check_parameters(ctx, &mut findings);
    Report {
        presentation: ctx.pres.name.clone(),
        context: ctx.spec.print(),
        findings,
    }
}

fn check_parameters<R: Ring>(ctx: &Context<R>, findings: &mut Vec<Finding>) {
    for p in &ctx.pres.parameters {
        let Some(table) = ctx.params.get(&p.name) else {
            continue;
        };
        for h in &p.hypotheses {
            let norm: String = h.chars().filter(|c| !c.is_whitespace()).collect();
            let check = format!("parameter {}: {h}", p.name);
            if norm == format!("{0}_ab(u,v)={0}_ba(v,u)", p.name) {
                let bad = table.iter().find(|((a, b), q)| {
                    table
                        .get(&(*b, *a))
                        .is_none_or(|r| ctx.params_ring.swap_vars(r, 0, 1) != **q)
                });
                match bad {
                    None => findings.push(finding(check, true, "")),
                    Some(((a, b), _)) => findings.push(finding(
                        check,
                        false,
                        format!(
                            "fails for a={}, b={}",
                            ctx.colours[*a as usize].name, ctx.colours[*b as usize].name
                        ),
                    )),
                }
            } else if norm == format!("{}_ab!=0", p.name) {
                let bad = table.iter().find(|(_, q)| ctx.params_ring.is_zero(q));
                findings.push(finding(check, bad.is_none(), ""));
            } else {
                findings.push(Finding {
                    check,
                    status: Status::Unchecked,
                    detail: "stated, not checked".into(),
                });
            }
        }
    }
}

/// The contexts a shipped presentation is checked in: every Cartan type of rank at most three
/// (plus `G2`) for Cartan presentations, the presentation alone otherwise. A presentation
/// whose regions carry weights is checked at several rightmost weights in a few types.
pub fn standard_contexts(pres: &Presentation) -> Vec<ContextSpec> {
    let base = ContextSpec {
        presentation: pres.name.clone(),
        ring: "QQ".into(),
        ..Default::default()
    };
    if uses_weights(pres) {
        let at = |t: &str, mus: &[&str]| -> Vec<ContextSpec> {
            mus.iter()
                .map(|m| ContextSpec {
                    cartan_type: Some(t.to_string()),
                    region: Some(m.to_string()),
                    ..base.clone()
                })
                .collect()
        };
        let mut out = at("A1", &["-3", "-2", "-1", "0", "1", "2", "3"]);
        out.extend(at("A2", &["0", "i", "-j", "2i-j", "-i-2j"]));
        out.extend(at("B2", &["0", "i-j", "-2j"]));
        out.extend(at("G2", &["0", "i", "-j"]));
        return out;
    }
    match pres.colours {
        ColourSpec::Cartan => ["A1", "A2", "A3", "B2", "B3", "C3", "G2"]
            .iter()
            .map(|t| ContextSpec {
                cartan_type: Some(t.to_string()),
                ..base.clone()
            })
            .collect(),
        ColourSpec::Fixed(_) => vec![base],
    }
}

/// Whether a presentation's regions carry weights: some degree or relation depends on them, or
/// it has bubbles.
pub fn uses_weights(pres: &Presentation) -> bool {
    !pres.bubbles.is_empty()
        || pres.generators.iter().any(|g| g.degree.uses_pairing())
        || pres.relations.iter().any(|r| pres.relation_uses_region(r))
}

/// Checks a presentation over ℚ in its standard contexts.
pub fn check_presentation(pres: &Presentation) -> Result<Vec<Report>> {
    standard_contexts(pres)
        .into_iter()
        .map(|spec| Ok(check(&Context::new(pres.clone(), spec, Rationals)?)))
        .collect()
}

/// Checks every shipped presentation.
pub fn check_builtin() -> Result<Vec<Report>> {
    let mut out = Vec::new();
    for (name, _) in builtin::PRESENTATIONS {
        out.extend(check_presentation(&builtin::presentation(name)?)?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_presentations_conform() {
        for r in check_builtin().unwrap() {
            let fails: Vec<String> = r
                .failures()
                .map(|f| format!("{}: {}", f.check, f.detail))
                .collect();
            assert!(
                fails.is_empty(),
                "{} ({}):\n{}",
                r.presentation,
                r.context,
                fails.join("\n")
            );
            let pres = builtin::presentation(&r.presentation).unwrap();
            assert!(
                pres.representations.is_empty()
                    || r.findings.iter().any(|f| f.check.contains("holds in")),
                "{}",
                r.context
            );
        }
    }
}
