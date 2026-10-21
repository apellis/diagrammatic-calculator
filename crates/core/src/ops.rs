//! Operations: the typed requests every surface (command line, WebAssembly, Python, later MCP)
//! sends to the core, and their JSON results. The schemas are in `spec/ops/`.

use crate::builtin;
use crate::conformance::{self, Report, Status};
use crate::context::{Context, ContextSpec};
use crate::elab::{CTerm, Scope};
use crate::error::{Error, Result};
use crate::pres::Presentation;
use crate::render;
use crate::rep::{self, Evaluator, Provenance};
use crate::ring::{GaussianIntegers, Integers, PrimeField, Rationals, Ring};
use crate::text;
use crate::workspace::{CellBody, Workspace};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// Something to do with a context over whichever ground ring it names.
pub trait RingVisitor {
    type Out;
    fn visit<R: Ring>(self, base: R) -> Result<Self::Out>;
}

/// Runs a visitor over the ring named `ZZ`, `QQ`, `GF(p)` or `ZZ[I]`.
pub fn with_ring<V: RingVisitor>(name: &str, v: V) -> Result<V::Out> {
    match name {
        "ZZ" => v.visit(Integers),
        "QQ" => v.visit(Rationals),
        "ZZ[I]" => v.visit(GaussianIntegers),
        s => {
            let p = s
                .strip_prefix("GF(")
                .and_then(|r| r.strip_suffix(')'))
                .and_then(|p| p.parse::<u64>().ok())
                .ok_or_else(|| {
                    Error::invalid(format!("unknown ring '{s}': use ZZ, QQ, GF(p) or ZZ[I]"))
                })?;
            let f =
                PrimeField::new(p).ok_or_else(|| Error::invalid(format!("{p} is not a prime")))?;
            v.visit(f)
        }
    }
}

/// The presentation a context line names: a shipped one, or a file given by `pres_text`.
pub fn load_presentation(spec: &ContextSpec, pres_text: Option<&str>) -> Result<Presentation> {
    match pres_text {
        Some(t) => {
            let p = Presentation::from_toml(t)?;
            if p.name != spec.presentation {
                return Err(Error::invalid(format!(
                    "the context names {} but the file is {}",
                    spec.presentation, p.name
                )));
            }
            Ok(p)
        }
        None => builtin::presentation(&spec.presentation),
    }
}

fn provenance_json(p: &Provenance) -> Value {
    json!({
        "strategy": p.strategy.as_str(),
        "method": p.method,
        "certificate": {
            "status": p.certificate.status,
            "reference": p.certificate.reference,
            "hypotheses": p.certificate.hypotheses,
        }
    })
}

/// A context with its definitions and its chosen equality strategy.
pub struct Session<'a, R: Ring> {
    pub ctx: &'a Context<R>,
    pub defs: BTreeMap<String, CTerm<R>>,
    pub eval: Box<dyn Evaluator<R> + 'a>,
}

impl<'a, R: Ring> Session<'a, R> {
    /// Uses the representation named `strategy`, or the first one the presentation declares.
    pub fn new(ctx: &'a Context<R>, strategy: Option<&str>) -> Result<Self> {
        let decl = match strategy {
            Some(name) => ctx
                .pres
                .representations
                .iter()
                .find(|d| d.name == name)
                .ok_or_else(|| Error::invalid(format!("no representation named '{name}'")))?,
            None => ctx.pres.representations.first().ok_or_else(|| {
                Error::precondition(format!(
                    "presentation {} declares no representation",
                    ctx.pres.name
                ))
            })?,
        };
        Ok(Session {
            ctx,
            defs: BTreeMap::new(),
            eval: rep::evaluator(ctx, decl)?,
        })
    }

    pub fn term(&self, ast: &text::Ast) -> Result<CTerm<R>> {
        let scope = Scope {
            colour_vars: vec![],
            defs: Some(&self.defs),
        };
        self.ctx.elaborate(ast, &scope)
    }

    pub fn parse(&self, src: &str) -> Result<CTerm<R>> {
        self.term(&text::parse(src)?)
    }

    pub fn simplify(&self, t: &CTerm<R>) -> Result<Value> {
        let nf = self.eval.normal_form(t)?;
        Ok(json!({
            "normal_form": self.ctx.fmt_term(&nf),
            "terms": nf.len(),
            "provenance": provenance_json(self.eval.provenance()),
        }))
    }

    /// A named yes-or-no check; the witness is the normal form of what should vanish.
    pub fn check(&self, name: &str, args: &[CTerm<R>]) -> Result<Value> {
        let arity = |n: usize| {
            if args.len() == n {
                Ok(())
            } else {
                Err(Error::invalid(format!("{name} takes {n} argument(s)")))
            }
        };
        let c = self.ctx;
        let should_vanish = match name {
            "equal" => {
                arity(2)?;
                c.sub(&args[0], &args[1])
            }
            "zero" => {
                arity(1)?;
                args[0].clone()
            }
            "idempotent" => {
                arity(1)?;
                c.sub(&c.compose(&args[0], &args[0]), &args[0])
            }
            "orthogonal" => {
                arity(2)?;
                let ab = c.compose(&args[0], &args[1]);
                let ba = c.compose(&args[1], &args[0]);
                let nf_ab = self.eval.normal_form(&ab)?;
                if !nf_ab.is_zero() {
                    return Ok(self.verdict(
                        false,
                        Some(format!("first ∘ second = {}", c.fmt_term(&nf_ab))),
                    ));
                }
                ba
            }
            "homogeneous" => {
                arity(1)?;
                let nf = self.eval.normal_form(&args[0])?;
                let degs: std::collections::BTreeSet<i64> =
                    nf.terms.iter().map(|(w, _)| c.word_degree(w)).collect();
                let ok = degs.len() <= 1;
                let witness = (!ok).then(|| format!("degrees {:?}", degs));
                return Ok(self.verdict(ok, witness));
            }
            "central" | "primitive" => {
                return Err(Error::precondition(format!(
                    "{name} needs a basis of a finite-dimensional piece, which arrives in phase 2"
                )));
            }
            _ => return Err(Error::invalid(format!("unknown check '{name}'"))),
        };
        let nf = self.eval.normal_form(&should_vanish)?;
        let witness = (!nf.is_zero()).then(|| c.fmt_term(&nf));
        Ok(self.verdict(nf.is_zero(), witness))
    }

    fn verdict(&self, holds: bool, witness: Option<String>) -> Value {
        json!({ "holds": holds, "witness": witness, "provenance": provenance_json(self.eval.provenance()) })
    }

    pub fn render(&self, t: &CTerm<R>, format: &str) -> Result<String> {
        match format {
            "text" => Ok(self.ctx.fmt_term(t)),
            "tikz" => Ok(render::tikz(self.ctx, t)),
            "svg" => Ok(render::svg(self.ctx, t)),
            f => Err(Error::invalid(format!(
                "unknown format '{f}': use text, tikz or svg"
            ))),
        }
    }
}

fn error_json(e: &Error) -> Value {
    json!({ "status": "error", "kind": format!("{:?}", e.kind).to_lowercase(), "message": e.message })
}

/// A request naming a context: the common part of the term operations.
#[derive(Clone, Debug, Default)]
pub struct Request {
    pub context: String,
    /// The presentation file's text, when it is not a shipped one.
    pub presentation: Option<String>,
    pub strategy: Option<String>,
}

struct TermOp<'a> {
    req: &'a Request,
    op: &'a str,
    terms: &'a [String],
    format: Option<&'a str>,
}

impl RingVisitor for TermOp<'_> {
    type Out = Value;
    fn visit<R: Ring>(self, base: R) -> Result<Value> {
        let spec = ContextSpec::parse(&self.req.context)?;
        let pres = load_presentation(&spec, self.req.presentation.as_deref())?;
        let ctx = Context::new(pres, spec, base)?;
        let s = Session::new(&ctx, self.req.strategy.as_deref())?;
        let terms: Vec<CTerm<R>> = self
            .terms
            .iter()
            .map(|t| s.parse(t))
            .collect::<Result<_>>()?;
        match self.op {
            "simplify" => {
                let [t] = terms.as_slice() else {
                    return Err(Error::invalid("simplify takes one term"));
                };
                s.simplify(t)
            }
            "render" => {
                let [t] = terms.as_slice() else {
                    return Err(Error::invalid("render takes one term"));
                };
                Ok(
                    json!({ "format": self.format.unwrap_or("text"), "output": s.render(t, self.format.unwrap_or("text"))? }),
                )
            }
            name => s.check(name, &terms),
        }
    }
}

fn with_status(r: Result<Value>) -> Value {
    match r {
        Ok(mut v) => {
            v.as_object_mut()
                .map(|o| o.insert("status".into(), "ok".into()));
            v
        }
        Err(e) => error_json(&e),
    }
}

fn ring_of(context: &str) -> Result<String> {
    Ok(ContextSpec::parse(context)?.ring)
}

/// `simplify`: the normal form of a term.
pub fn simplify(req: &Request, term: &str) -> Value {
    with_status(ring_of(&req.context).and_then(|r| {
        with_ring(
            &r,
            TermOp {
                req,
                op: "simplify",
                terms: &[term.to_string()],
                format: None,
            },
        )
    }))
}

/// `check`: a named check (`equal`, `zero`, `idempotent`, `orthogonal`, `homogeneous`).
pub fn check(req: &Request, name: &str, terms: &[String]) -> Value {
    with_status(ring_of(&req.context).and_then(|r| {
        with_ring(
            &r,
            TermOp {
                req,
                op: name,
                terms,
                format: None,
            },
        )
    }))
}

/// `term.render`: a term as text, TikZ or SVG.
pub fn render(req: &Request, term: &str, format: &str) -> Value {
    with_status(ring_of(&req.context).and_then(|r| {
        with_ring(
            &r,
            TermOp {
                req,
                op: "render",
                terms: &[term.to_string()],
                format: Some(format),
            },
        )
    }))
}

fn report_json(r: &Report) -> Value {
    let count = |s: Status| r.findings.iter().filter(|f| f.status == s).count();
    json!({
        "presentation": r.presentation,
        "context": r.context,
        "ok": r.ok(),
        "passed": count(Status::Pass),
        "failed": count(Status::Fail),
        "unchecked": count(Status::Unchecked),
        "findings": r.findings.iter().filter(|f| f.status != Status::Pass).map(|f| json!({
            "check": f.check,
            "status": format!("{:?}", f.status).to_lowercase(),
            "detail": f.detail,
        })).collect::<Vec<_>>(),
    })
}

/// `presentation.check`: the conformance checker, in the given context or the standard ones.
pub fn presentation_check(name_or_text: &str, context: Option<&str>) -> Value {
    let run = || -> Result<Value> {
        let pres = if name_or_text.contains('\n') {
            Presentation::from_toml(name_or_text)?
        } else {
            builtin::presentation(name_or_text)?
        };
        let reports = match context {
            Some(c) => {
                let spec = ContextSpec::parse(c)?;
                struct V<'a>(&'a Presentation, ContextSpec);
                impl RingVisitor for V<'_> {
                    type Out = Report;
                    fn visit<R: Ring>(self, base: R) -> Result<Report> {
                        Ok(conformance::check(&Context::new(
                            self.0.clone(),
                            self.1,
                            base,
                        )?))
                    }
                }
                let ring = spec.ring.clone();
                vec![with_ring(&ring, V(&pres, spec))?]
            }
            None => conformance::check_presentation(&pres)?,
        };
        let ok = reports.iter().all(Report::ok);
        Ok(json!({ "ok": ok, "reports": reports.iter().map(report_json).collect::<Vec<_>>() }))
    };
    with_status(run())
}

/// The result of one workspace cell.
#[derive(Clone, Debug, Serialize)]
pub struct CellResult {
    pub id: Option<String>,
    pub kind: &'static str,
    #[serde(flatten)]
    pub value: Value,
}

struct RunWorkspace<'a> {
    ws: &'a Workspace,
    presentation: Option<&'a str>,
}

impl RingVisitor for RunWorkspace<'_> {
    type Out = Vec<CellResult>;
    fn visit<R: Ring>(self, base: R) -> Result<Vec<CellResult>> {
        let spec = self.ws.context.clone();
        let pres = load_presentation(&spec, self.presentation)?;
        let ctx = Context::new(pres, spec, base)?;
        let mut s = Session::new(&ctx, None)?;
        let mut out = Vec::new();
        for cell in &self.ws.cells {
            let strategy = cell
                .options
                .iter()
                .find(|(k, _)| k == "strategy")
                .map(|(_, v)| v.as_str());
            if let Some(name) = strategy {
                s.eval = rep::evaluator(
                    &ctx,
                    ctx.pres
                        .representations
                        .iter()
                        .find(|d| d.name == name)
                        .ok_or_else(|| {
                            Error::invalid(format!("no representation named '{name}'"))
                        })?,
                )?;
            }
            let (kind, r): (&str, Result<Value>) = match &cell.body {
                CellBody::Def { name, term } => (
                    "def",
                    s.term(term).map(|t| {
                        let v = json!({ "name": name, "term": ctx.fmt_term(&t) });
                        s.defs.insert(name.clone(), t);
                        v
                    }),
                ),
                CellBody::Watch { term } => ("watch", s.term(term).and_then(|t| s.simplify(&t))),
                CellBody::Check { name, args } => (
                    "check",
                    args.iter()
                        .map(|a| s.term(a))
                        .collect::<Result<Vec<_>>>()
                        .and_then(|ts| s.check(name, &ts)),
                ),
                CellBody::Solve { .. } => (
                    "solve",
                    Err(Error::precondition(
                        "solving arrives in phase 2; this cell is kept but not run",
                    )),
                ),
                CellBody::Note { .. } | CellBody::Comment(_) | CellBody::Blank => continue,
            };
            out.push(CellResult {
                id: cell.id.clone(),
                kind,
                value: with_status(r),
            });
            if strategy.is_some() {
                s.eval = Session::new(&ctx, None)?.eval;
            }
        }
        Ok(out)
    }
}

/// `workspace.run`: evaluates every cell of a workspace in order.
pub fn run_workspace(src: &str, presentation: Option<&str>) -> Value {
    let run = || -> Result<Value> {
        let ws = Workspace::parse(src)?;
        let ring = ws.context.ring.clone();
        let cells = with_ring(
            &ring,
            RunWorkspace {
                ws: &ws,
                presentation,
            },
        )?;
        Ok(json!({ "context": ws.context.print(), "cells": cells }))
    };
    with_status(run())
}

/// The operations every surface exposes, by name.
pub const OPERATIONS: [&str; 5] = [
    "simplify",
    "check",
    "term.render",
    "presentation.check",
    "workspace.run",
];

/// Runs an operation from its JSON request (see `spec/ops/`), returning its JSON response.
pub fn call(op: &str, input: &Value) -> Value {
    let str_field = |k: &str| input.get(k).and_then(Value::as_str).map(String::from);
    let req = || -> Result<Request> {
        Ok(Request {
            context: str_field("context")
                .ok_or_else(|| Error::invalid("the request needs a context"))?,
            presentation: str_field("presentation"),
            strategy: str_field("strategy"),
        })
    };
    let need =
        |k: &str| str_field(k).ok_or_else(|| Error::invalid(format!("the request needs '{k}'")));
    let r: Result<Value> = (|| {
        Ok(match op {
            "simplify" => simplify(&req()?, &need("term")?),
            "check" => {
                let terms: Vec<String> = input
                    .get("terms")
                    .and_then(Value::as_array)
                    .ok_or_else(|| Error::invalid("check needs 'terms', a list of terms"))?
                    .iter()
                    .map(|t| {
                        t.as_str()
                            .map(String::from)
                            .ok_or_else(|| Error::invalid("terms are strings"))
                    })
                    .collect::<Result<_>>()?;
                check(&req()?, &need("name")?, &terms)
            }
            "term.render" => render(
                &req()?,
                &need("term")?,
                &str_field("format").unwrap_or("text".into()),
            ),
            "presentation.check" => {
                presentation_check(&need("presentation")?, str_field("context").as_deref())
            }
            "workspace.run" => {
                run_workspace(&need("workspace")?, str_field("presentation").as_deref())
            }
            _ => return Err(Error::invalid(format!("unknown operation '{op}'"))),
        })
    })();
    match r {
        Ok(v) => v,
        Err(e) => error_json(&e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(c: &str) -> Request {
        Request {
            context: c.into(),
            ..Default::default()
        }
    }

    #[test]
    fn simplify_and_check() {
        let v = simplify(&req("nilhecke nu=2 ring=ZZ"), "x_1 psi_1");
        assert_eq!(v["status"], "ok");
        assert_eq!(v["normal_form"], "1 + psi_1 x_2");
        assert_eq!(v["provenance"]["strategy"], "faithful representation");
        let v = check(
            &req("klr type=A2 nu=2i+j"),
            "idempotent",
            &["psi_1 psi_2 psi_1 e(iji)".into()],
        );
        assert_eq!(v["holds"], true, "{v}");
        let v = check(
            &req("klr type=A2 nu=2i+j"),
            "idempotent",
            &["psi_1 e(iji)".into()],
        );
        assert_eq!(v["holds"], false, "{v}");
        let v = simplify(&req("klr type=A2 nu=2i+j ring=GF(4)"), "x_1");
        assert_eq!(v["status"], "error");
    }

    #[test]
    fn workspace_runs() {
        let src = "diagcalc-workspace/1
context nilhecke nu=3 ring=QQ
def a := psi_1 psi_2 psi_1 x_1^2 x_2  # id=d1
watch a a - a  # id=w2
check idempotent(a)  # id=k3
check equal(x_1 psi_1, psi_1 x_2 + 1)  strategy=nil-coxeter-basis  # id=k4
solve a = 0 for c  # id=s5
";
        let v = run_workspace(src, None);
        assert_eq!(v["status"], "ok", "{v}");
        let cells = v["cells"].as_array().unwrap();
        assert_eq!(cells[1]["normal_form"], "0", "{v}");
        assert_eq!(cells[2]["holds"], true);
        assert_eq!(cells[3]["holds"], true);
        assert_eq!(cells[3]["provenance"]["method"], "nil-coxeter-basis");
        assert_eq!(cells[4]["status"], "error");
    }

    #[test]
    fn json_calls() {
        let v = call(
            "simplify",
            &json!({ "context": "nilhecke nu=2", "term": "x_1 psi_1" }),
        );
        assert_eq!(v["normal_form"], "1 + psi_1 x_2");
        let v = call(
            "check",
            &json!({ "context": "nilhecke nu=2", "name": "zero", "terms": ["psi_1 psi_1"] }),
        );
        assert_eq!(v["holds"], true);
        let v = call("nonsense", &json!({}));
        assert_eq!(v["status"], "error");
        let v = call("simplify", &json!({ "term": "x_1" }));
        assert_eq!(v["message"], "the request needs a context");
    }

    #[test]
    fn presentation_check_runs() {
        let v = presentation_check("odd-nilhecke", None);
        assert_eq!(v["ok"], true, "{v}");
        let v = presentation_check("klr", Some("klr type=B2 ring=GF(7)"));
        assert_eq!(v["ok"], true, "{v}");
    }
}
