//! Operations: the typed requests every surface (command line, WebAssembly, Python, later MCP)
//! sends to the core, and their JSON results. The schemas are in `spec/ops/`.

use crate::builtin;
use crate::conformance::{self, Report, Status};
use crate::context::{Context, ContextSpec};
use crate::elab::{CTerm, Scope};
use crate::error::{Error, Result};
use crate::groebner::Limits;
use crate::poly::PolyRing;
use crate::pres::Presentation;
use crate::quotient;
use crate::ratfun::RationalFunctions;
use crate::render;
use crate::rep::{self, Evaluator, Provenance};
use crate::ring::{GaussianIntegers, Integers, PrimeField, Rationals, Ring};
use crate::solve;
use crate::text;
use crate::toolkit;
use crate::workspace::{Cell, CellBody, Workspace};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// Something to do with a context over whichever ground ring it names.
pub trait RingVisitor {
    type Out;
    fn visit<R: Ring>(self, base: R) -> Result<Self::Out>;
}

/// Runs a visitor over the ring named `ZZ`, `QQ`, `GF(p)`, `ZZ[I]`, a polynomial ring `ZZ[q]` or
/// `QQ[q]`, a Laurent polynomial ring `ZZ[q,q^-1]` or `QQ[q,q^-1]`, or a rational function field
/// `QQ(q)`, for a variable name `q` of lowercase letters other than `u`, `v`, `w`, `x` and `e`.
pub fn with_ring<V: RingVisitor>(name: &str, v: V) -> Result<V::Out> {
    let name: String = name.chars().filter(|c| !c.is_whitespace()).collect();
    let unknown = || {
        Error::invalid(format!(
            "unknown ring '{name}': use ZZ, QQ, GF(p), ZZ[I], ZZ[q], QQ[q], ZZ[q,q^-1], \
             QQ[q,q^-1] or QQ(q)"
        ))
    };
    let var_ok = |s: &str| {
        !s.is_empty()
            && s.chars().all(|c| c.is_ascii_lowercase())
            && !["u", "v", "w", "x", "e"].contains(&s)
    };
    match name.as_str() {
        "ZZ" => return v.visit(Integers),
        "QQ" => return v.visit(Rationals),
        "ZZ[I]" => return v.visit(GaussianIntegers),
        _ => {}
    }
    if let Some(p) = name.strip_prefix("GF(").and_then(|r| r.strip_suffix(')')) {
        let p = p.parse::<u64>().map_err(|_| unknown())?;
        let f = PrimeField::new(p).ok_or_else(|| Error::invalid(format!("{p} is not a prime")))?;
        return v.visit(f);
    }
    if let Some(var) = name.strip_prefix("QQ(").and_then(|r| r.strip_suffix(')')) {
        if !var_ok(var) {
            return Err(unknown());
        }
        return v.visit(RationalFunctions::new(Rationals, var));
    }
    let (base, inside) = name
        .split_once('[')
        .and_then(|(b, r)| r.strip_suffix(']').map(|r| (b, r)))
        .ok_or_else(unknown)?;
    let (var, laurent) = match inside.split_once(',') {
        Some((a, b)) if b == format!("{a}^-1") => (a, true),
        Some(_) => return Err(unknown()),
        None => (inside, false),
    };
    if !var_ok(var) {
        return Err(unknown());
    }
    match base {
        "ZZ" => {
            let mut r = PolyRing::new(Integers, &[var]);
            r.laurent[0] = laurent;
            v.visit(r)
        }
        "QQ" => {
            let mut r = PolyRing::new(Rationals, &[var]);
            r.laurent[0] = laurent;
            v.visit(r)
        }
        _ => Err(unknown()),
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

fn provenance_json<R: Ring>(eval: &dyn Evaluator<R>) -> Value {
    let p: &Provenance = eval.provenance();
    let mut v = json!({
        "strategy": p.strategy.as_str(),
        "method": p.method,
        "certificate": {
            "status": p.certificate.status,
            "reference": p.certificate.reference,
            "hypotheses": p.certificate.hypotheses,
        }
    });
    let notes = eval.notes();
    if !notes.is_empty() {
        v["notes"] = json!(notes);
    }
    v
}

/// The evaluator for a representation declaration, inside the quotient the context names.
fn evaluator_for<'a, R: Ring + 'a>(
    ctx: &'a Context<R>,
    decl: &crate::pres::RepresentationDecl,
) -> Result<Box<dyn Evaluator<R> + 'a>> {
    quotient::for_context(ctx, rep::evaluator(ctx, decl)?)
}

/// A context with its definitions and its chosen equality strategy.
pub struct Session<'a, R: Ring> {
    pub ctx: &'a Context<R>,
    pub defs: BTreeMap<String, CTerm<R>>,
    pub eval: Box<dyn Evaluator<R> + 'a>,
}

impl<'a, R: Ring> Session<'a, R> {
    /// Uses the representation named `strategy`, or the first one the presentation declares that
    /// applies in the context.
    pub fn new(ctx: &'a Context<R>, strategy: Option<&str>) -> Result<Self> {
        let decl = match strategy {
            Some(name) => ctx
                .pres
                .representations
                .iter()
                .find(|d| d.name == name)
                .ok_or_else(|| Error::invalid(format!("no representation named '{name}'")))?,
            None => rep::first_applicable(ctx)?,
        };
        Ok(Session {
            ctx,
            defs: BTreeMap::new(),
            eval: evaluator_for(ctx, decl)?,
        })
    }

    pub fn term(&self, ast: &text::Ast) -> Result<CTerm<R>> {
        let scope = Scope {
            defs: Some(&self.defs),
            ..Scope::default()
        };
        self.ctx.elaborate(ast, &scope)
    }

    pub fn parse(&self, src: &str) -> Result<CTerm<R>> {
        self.term(&text::parse(src)?)
    }

    pub fn simplify(&self, t: &CTerm<R>) -> Result<Value> {
        self.simplify_shown(t, None)
    }

    /// `simplify`, with the first `shown` summands drawn when it is given.
    pub fn simplify_shown(&self, t: &CTerm<R>, shown: Option<usize>) -> Result<Value> {
        crate::progress::stage("normal form");
        self.ctx.report_sizes(t);
        let nf = self.eval.normal_form(t)?;
        self.ctx.report_sizes(&nf);
        // a normal form of zero proves the term zero when every step is a relation; that a
        // nonzero normal form is the term's own (and nonzero) rests on the certificate
        let proven = nf.is_zero() && self.eval.rewrites_by_relations();
        let mut v = json!({
            "normal_form": self.ctx.fmt_term(&nf),
            "terms": nf.len(),
            "certified_by": if proven { "computation" } else { "certificate" },
            "provenance": provenance_json(self.eval.as_ref()),
        });
        if let Some(n) = shown {
            v["summands"] = summands_json(self.ctx, &nf, n);
        }
        Ok(v)
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
            "central" => {
                arity(1)?;
                let z = self.eval.normal_form(&args[0])?;
                if let Some((w, _)) = z.terms.iter().find(|(w, _)| c.target(w) != w.source) {
                    return Ok(self.verdict(
                        false,
                        Some(format!(
                            "{} does not commute with the idempotents",
                            c.fmt_word(w)
                        )),
                    ));
                }
                for g in toolkit::generator_layers(c) {
                    let gt = c.monomial_term(&g, c.coeff.one());
                    let comm = self
                        .eval
                        .normal_form(&c.sub(&c.compose(&z, &gt), &c.compose(&gt, &z)))?;
                    if !comm.is_zero() {
                        return Ok(self.verdict(
                            false,
                            Some(format!(
                                "with {}: a g - g a = {}",
                                c.fmt_word(&g),
                                c.fmt_term(&comm)
                            )),
                        ));
                    }
                }
                return Ok(self.verdict(true, None));
            }
            "primitive" => {
                arity(1)?;
                let alg = toolkit::Ambient {
                    ctx: c,
                    eval: self.eval.as_ref(),
                };
                let ok = toolkit::is_primitive(&alg, &args[0])?;
                let witness = (!ok).then(|| "e A e / e J e has dimension greater than 1".into());
                // a dimension count rests on the basis either way
                return Ok(self.verdict_by(ok, witness, false));
            }
            _ => return Err(Error::invalid(format!("unknown check '{name}'"))),
        };
        let nf = self.eval.normal_form(&should_vanish)?;
        let witness = (!nf.is_zero()).then(|| c.fmt_term(&nf));
        Ok(self.verdict(nf.is_zero(), witness))
    }

    /// A verdict read off normal forms: it holds when they vanish (or, for `homogeneous` and
    /// `central`, have the required shape), which the computation alone proves when every step is a
    /// relation; that a check fails rests on the certificate (distinct normal forms differ).
    fn verdict(&self, holds: bool, witness: Option<String>) -> Value {
        let proven = holds && self.eval.rewrites_by_relations();
        self.verdict_by(holds, witness, proven)
    }

    /// A verdict, with what it rests on: `computation` when the computation alone proves it,
    /// `certificate` when it also rests on the representation's certificate.
    fn verdict_by(&self, holds: bool, witness: Option<String>, proven: bool) -> Value {
        json!({
            "holds": holds,
            "witness": witness,
            "certified_by": if proven { "computation" } else { "certificate" },
            "provenance": provenance_json(self.eval.as_ref()),
        })
    }

    pub fn render(&self, t: &CTerm<R>, format: &str) -> Result<String> {
        render_term(self.ctx, t, format)
    }
}

/// A term as text, TikZ or SVG.
pub fn render_term<R: Ring>(ctx: &Context<R>, t: &CTerm<R>, format: &str) -> Result<String> {
    match format {
        "text" => Ok(ctx.fmt_term(t)),
        "tikz" => Ok(render::tikz(ctx, t)),
        "svg" => Ok(render::svg(ctx, t)),
        f => Err(Error::invalid(format!(
            "unknown format '{f}': use text, tikz or svg"
        ))),
    }
}

/// The first `shown` summands of a term for display, in printed order: each with its signed text
/// (as in the term's text form), its word as a key, its degree, and a drawing of the word in its
/// canonical layering with the sign and coefficient that go with that drawing.
pub fn summands_json<R: Ring>(ctx: &Context<R>, t: &CTerm<R>, shown: usize) -> Value {
    let one = ctx.coeff.one();
    let items: Vec<Value> = t
        .terms
        .iter()
        .take(shown)
        .map(|(w, c)| {
            let single = crate::term::Term {
                terms: vec![(w.clone(), c.clone())],
            };
            let neg = ctx.coeff.is_negative(c);
            let abs = if neg { ctx.coeff.neg(c) } else { c.clone() };
            let coefficient = (!ctx.coeff.is_one(&abs)).then(|| {
                let cs = ctx.coeff.fmt_elem(&abs);
                if ctx.coeff.needs_parens(&abs) {
                    format!("({cs})")
                } else {
                    cs
                }
            });
            json!({
                "text": ctx.fmt_term(&single),
                "word": ctx.fmt_word(w),
                "degree": ctx.word_degree(w),
                "source": ctx.fmt_object(&w.source),
                "target": ctx.fmt_object(&ctx.target(w)),
                "negative": neg,
                "coefficient": coefficient,
                "svg": render::svg(ctx, &ctx.monomial_term(w, one.clone())),
            })
        })
        .collect();
    json!(items)
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
        if self.op == "render" {
            // Drawing needs no equality strategy.
            let [t] = self.terms else {
                return Err(Error::invalid("render takes one term"));
            };
            let t = ctx.elaborate(&text::parse(t)?, &Scope::default())?;
            let format = self.format.unwrap_or("text");
            let output = render_term(&ctx, &t, format)?;
            return Ok(json!({ "format": format, "output": output }));
        }
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
            name => s.check(name, &terms),
        }
    }
}

pub(crate) fn with_status(r: Result<Value>) -> Value {
    match r {
        Ok(mut v) => {
            v.as_object_mut()
                .map(|o| o.insert("status".into(), "ok".into()));
            v
        }
        Err(e) => error_json(&e),
    }
}

pub(crate) fn ring_of(context: &str) -> Result<String> {
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
    /// Draw up to this many summands of each definition and watch.
    shown: Option<usize>,
    /// Scalars to add to the context, such as the unknowns of solve cells elsewhere.
    scalars: &'a [String],
}

/// The definitions and unknowns each cell of a workspace depends on.
struct Dependencies {
    /// For each cell, the definitions its terms use: the latest one of each name before it.
    defs: Vec<Vec<usize>>,
    /// For each cell, the unknowns of solve cells that it or the definitions it uses mention.
    unknowns: Vec<Vec<String>>,
}

impl Dependencies {
    fn of(ws: &Workspace) -> Dependencies {
        // the unknowns of all solve cells, in order of first appearance
        let mut all: Vec<String> = Vec::new();
        for cell in &ws.cells {
            if let CellBody::Solve { unknowns, .. } = &cell.body {
                for u in unknowns {
                    if !all.contains(u) {
                        all.push(u.clone());
                    }
                }
            }
        }
        let n = ws.cells.len();
        let mut defs = vec![Vec::new(); n];
        let mut needs: Vec<std::collections::BTreeSet<String>> = vec![Default::default(); n];
        let mut latest: BTreeMap<String, usize> = BTreeMap::new();
        for (i, cell) in ws.cells.iter().enumerate() {
            for name in cell.names_used() {
                if let Some(&j) = latest.get(&name) {
                    defs[i].push(j);
                    let nj = needs[j].clone();
                    needs[i].extend(nj);
                }
                if all.contains(&name) {
                    needs[i].insert(name);
                }
            }
            if let CellBody::Solve { unknowns, .. } = &cell.body {
                needs[i].extend(unknowns.iter().cloned());
            }
            if let Some(name) = cell.defines() {
                latest.insert(name.to_string(), i);
            }
        }
        let unknowns = needs
            .iter()
            .map(|need| all.iter().filter(|u| need.contains(*u)).cloned().collect())
            .collect();
        Dependencies { defs, unknowns }
    }
}

/// A session for one set of scalars, with the definitions elaborated in it so far.
struct Scoped<'c, R: Ring> {
    s: Session<'c, R>,
    terms: BTreeMap<usize, Result<CTerm<R>>>,
}

impl<R: Ring> Scoped<'_, R> {
    /// Binds the names of the definitions cell `i` uses, elaborating them first.
    fn bind(&mut self, ws: &Workspace, deps: &Dependencies, i: usize) -> Result<()> {
        let mut defs = BTreeMap::new();
        for &j in &deps.defs[i] {
            let t = self.def(ws, deps, j)?;
            defs.insert(ws.cells[j].defines().unwrap_or_default().to_string(), t);
        }
        self.s.defs = defs;
        Ok(())
    }

    /// The term of the definition in cell `j`.
    fn def(&mut self, ws: &Workspace, deps: &Dependencies, j: usize) -> Result<CTerm<R>> {
        if let Some(t) = self.terms.get(&j) {
            return t.clone();
        }
        let CellBody::Def { name, term } = &ws.cells[j].body else {
            return Err(Error::invalid("not a definition"));
        };
        let t = self
            .bind(ws, deps, j)
            .and_then(|_| self.s.term(term))
            .map_err(|e| Error {
                message: format!("in the definition of {name}: {}", e.message),
                ..e
            });
        self.terms.insert(j, t.clone());
        t
    }
}

impl RingVisitor for RunWorkspace<'_> {
    type Out = Vec<CellResult>;
    fn visit<R: Ring>(self, base: R) -> Result<Vec<CellResult>> {
        // Each cell runs in the context with the unknowns that it, or the definitions it uses,
        // mention: an unknown of some solve cell never turns the coefficients of an unrelated
        // cell into polynomials.
        let ws = self.ws;
        let deps = Dependencies::of(ws);
        let mut sets: Vec<Vec<String>> = vec![vec![]];
        let which: Vec<usize> = deps
            .unknowns
            .iter()
            .map(|u| match sets.iter().position(|s| s == u) {
                Some(k) => k,
                None => {
                    sets.push(u.clone());
                    sets.len() - 1
                }
            })
            .collect();
        let pres = load_presentation(&ws.context, self.presentation)?;
        let ctxs: Vec<Context<R>> = sets
            .iter()
            .map(|set| {
                let mut spec = ws.context.clone();
                for u in self.scalars.iter().chain(set) {
                    if !spec.scalars.contains(u) {
                        spec.scalars.push(u.clone());
                    }
                }
                Context::new(pres.clone(), spec, base.clone())
            })
            .collect::<Result<_>>()?;
        let mut scoped: Vec<Scoped<R>> = ctxs
            .iter()
            .map(|c| {
                Ok(Scoped {
                    s: Session::new(c, None)?,
                    terms: BTreeMap::new(),
                })
            })
            .collect::<Result<_>>()?;
        let mut out = Vec::new();
        for (i, cell) in ws.cells.iter().enumerate() {
            crate::progress::begin("elaborating", cell.id.as_deref());
            let sc = &mut scoped[which[i]];
            let ctx = sc.s.ctx;
            let strategy = cell
                .options
                .iter()
                .find(|(k, _)| k == "strategy")
                .map(|(_, v)| v.as_str());
            if let Some(name) = strategy {
                sc.s.eval = evaluator_for(
                    ctx,
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
                CellBody::Def { name, .. } => (
                    "def",
                    sc.def(ws, &deps, i).map(|t| {
                        let mut v =
                            json!({ "name": name, "term": ctx.fmt_term(&t), "terms": t.len() });
                        if let Some(n) = self.shown {
                            v["summands"] = summands_json(ctx, &t, n);
                        }
                        v
                    }),
                ),
                CellBody::Watch { term } => (
                    "watch",
                    sc.bind(ws, &deps, i)
                        .and_then(|_| sc.s.term(term))
                        .and_then(|t| {
                            let mut v = sc.s.simplify_shown(&t, self.shown)?;
                            v["term"] = json!(ctx.fmt_term(&t));
                            Ok(v)
                        }),
                ),
                CellBody::Check { name, args } => (
                    "check",
                    sc.bind(ws, &deps, i).and_then(|_| {
                        args.iter()
                            .map(|a| sc.s.term(a))
                            .collect::<Result<Vec<_>>>()
                            .and_then(|ts| sc.s.check(name, &ts))
                    }),
                ),
                CellBody::Solve { lhs, rhs, unknowns } => (
                    "solve",
                    (|| {
                        sc.bind(ws, &deps, i)?;
                        let t = ctx.sub(&sc.s.term(lhs)?, &sc.s.term(rhs)?);
                        let nf = sc.s.eval.normal_form(&t)?;
                        let mut v = solve::solve(ctx, &nf, unknowns, &Limits::default())?;
                        v["provenance"] = provenance_json(sc.s.eval.as_ref());
                        Ok(v)
                    })(),
                ),
                CellBody::Note { .. } | CellBody::Comment(_) | CellBody::Blank => continue,
            };
            let mut value = with_status(r);
            // pieces that are 0 because their object is not the context's: said, not silent
            let asts: Vec<&crate::text::Ast> = match &cell.body {
                CellBody::Def { term, .. } | CellBody::Watch { term } => vec![term],
                CellBody::Check { args, .. } => args.iter().collect(),
                CellBody::Solve { lhs, rhs, .. } => vec![lhs, rhs],
                _ => vec![],
            };
            let mut outside: Vec<String> = Vec::new();
            for a in asts {
                for w in ctx.objects_outside(a) {
                    if !outside.contains(&w) {
                        outside.push(w);
                    }
                }
            }
            if value["status"] == "ok" && !outside.is_empty() {
                let of = match &ctx.spec.weight {
                    Some(nu) => format!("of weight $\\nu = {nu}$"),
                    None => "of this context".to_string(),
                };
                value["warnings"] = json!(
                    outside
                        .iter()
                        .map(|w| format!("${w}$ is $0$ here: its object is not one {of}"))
                        .collect::<Vec<_>>()
                );
            }
            // the cell's kind takes the key; an error's kind moves to error_kind
            if let Some(o) = value.as_object_mut()
                && let Some(k) = o.remove("kind")
            {
                o.insert("error_kind".into(), k);
            }
            out.push(CellResult {
                id: cell.id.clone(),
                kind,
                value,
            });
            if strategy.is_some() {
                sc.s.eval = Session::new(ctx, None)?.eval;
            }
        }
        Ok(out)
    }
}

/// `workspace.run`: evaluates every cell of a workspace in order.
pub fn run_workspace(src: &str, presentation: Option<&str>) -> Value {
    run_workspace_with(src, presentation, None, &[])
}

/// `workspace.run`, drawing up to `shown` summands of each definition and watch, with extra
/// scalars in the context.
pub fn run_workspace_with(
    src: &str,
    presentation: Option<&str>,
    shown: Option<usize>,
    scalars: &[String],
) -> Value {
    let run = || -> Result<Value> {
        let ws = Workspace::parse(src)?;
        let ring = ws.context.ring.clone();
        let cells = with_ring(
            &ring,
            RunWorkspace {
                ws: &ws,
                presentation,
                shown,
                scalars,
            },
        )?;
        Ok(json!({ "context": ws.context.print(), "cells": cells }))
    };
    with_status(run())
}

/// The most terms of the definitions a watch uses that `workspace.trace` follows.
pub const TRACE_TERMS: usize = 120;

/// `workspace.trace`: which terms of the definitions a watch uses produced one summand of its
/// value. Each term of each definition the watch names is multiplied by its own new scalar (a
/// tag); the watch is evaluated with them, and the coefficient of the summand, a polynomial in the
/// tags, is split by its monomials in them: each monomial names the terms of the definitions
/// (with multiplicity) whose product contributed, and its coefficient is that contribution. The
/// contributions add up to the summand. Monomials without tags come from the watch's own terms.
struct TraceWorkspace<'a> {
    ws: &'a Workspace,
    presentation: Option<&'a str>,
    scalars: &'a [String],
    cell: &'a str,
    word: &'a str,
}

impl RingVisitor for TraceWorkspace<'_> {
    type Out = Value;
    fn visit<R: Ring>(self, base: R) -> Result<Value> {
        let ws = self.ws;
        let deps = Dependencies::of(ws);
        let i = ws
            .cells
            .iter()
            .position(|c| c.id.as_deref() == Some(self.cell))
            .ok_or_else(|| Error::invalid(format!("there is no cell '{}'", self.cell)))?;
        let CellBody::Watch { term } = &ws.cells[i].body else {
            return Err(Error::invalid("only the summands of a watch are traced"));
        };
        let mut direct = deps.defs[i].clone();
        direct.sort_unstable();
        direct.dedup();
        let pres = load_presentation(&ws.context, self.presentation)?;
        let mut spec = ws.context.clone();
        for u in self.scalars.iter().chain(&deps.unknowns[i]) {
            if !spec.scalars.contains(u) {
                spec.scalars.push(u.clone());
            }
        }
        let strategy = ws.cells[i]
            .options
            .iter()
            .find(|(k, _)| k == "strategy")
            .map(|(_, v)| v.clone());
        // the definitions as written, to count and print their terms
        let plain = Context::new(pres.clone(), spec.clone(), base.clone())?;
        let mut sc = Scoped {
            s: Session::new(&plain, None)?,
            terms: BTreeMap::new(),
        };
        let mut defs: Vec<(usize, CTerm<R>)> = Vec::new();
        for &j in &direct {
            defs.push((j, sc.def(ws, &deps, j)?));
        }
        let count: usize = defs.iter().map(|(_, t)| t.len()).sum();
        if count > TRACE_TERMS {
            return Err(Error::limit(format!(
                "the definitions this watch uses have {count} terms; tracing follows at most {TRACE_TERMS}"
            )));
        }
        // one tag per term, named so that no scalar of the workspace can be one
        let first_tag = spec.scalars.len();
        for k in 0..count {
            spec.scalars.push(format!("trace__{k}"));
        }
        let ctx = Context::new(pres, spec, base)?;
        let mut sc = Scoped {
            s: Session::new(&ctx, None)?,
            terms: BTreeMap::new(),
        };
        if let Some(name) = &strategy {
            sc.s.eval = evaluator_for(
                &ctx,
                ctx.pres
                    .representations
                    .iter()
                    .find(|d| &d.name == name)
                    .ok_or_else(|| Error::invalid(format!("no representation named '{name}'")))?,
            )?;
        }
        let mut tagged = BTreeMap::new();
        let mut labels: Vec<Value> = Vec::new();
        let mut k = 0;
        for (j, written) in &defs {
            let t = sc.def(ws, &deps, *j)?;
            let name = ws.cells[*j].defines().unwrap_or_default().to_string();
            let mut terms = Vec::new();
            for (n, (w, c)) in t.terms.iter().enumerate() {
                let tag = ctx.coeff.var(first_tag + k);
                terms.push((w.clone(), ctx.coeff.mul(c, &tag)));
                let shown = written
                    .terms
                    .get(n)
                    .cloned()
                    .unwrap_or((w.clone(), c.clone()));
                labels.push(json!({
                    "name": name,
                    "cell": ws.cells[*j].id,
                    "term": n + 1,
                    "text": plain.fmt_term(&crate::term::Term { terms: vec![shown] }),
                }));
                k += 1;
            }
            tagged.insert(name, crate::term::Term { terms });
        }
        sc.s.defs = tagged;
        let t = sc.s.term(term)?;
        let nf = sc.s.eval.normal_form(&t)?;
        let Some((w, c)) = nf.terms.iter().find(|(w, _)| ctx.fmt_word(w) == self.word) else {
            return Err(Error::invalid(format!(
                "the value of {} has no summand {}",
                self.cell, self.word
            )));
        };
        // split the coefficient by its exponents in the tags
        let mut parts: BTreeMap<Vec<i32>, Vec<(crate::poly::Monomial, R::Elem)>> = BTreeMap::new();
        let mut total = Vec::new();
        for (m, a) in &c.terms {
            let tags: Vec<i32> = m.0[first_tag..].to_vec();
            let mut rest = m.clone();
            for e in rest.0[first_tag..].iter_mut() {
                *e = 0;
            }
            total.push((rest.clone(), a.clone()));
            parts.entry(tags).or_default().push((rest, a.clone()));
        }
        let total = ctx.coeff.normalize(total);
        let sources: Vec<Value> = parts
            .into_iter()
            .rev()
            .map(|(tags, terms)| {
                let coeff = ctx.coeff.normalize(terms);
                let single = crate::term::Term {
                    terms: vec![(w.clone(), coeff)],
                };
                let factors: Vec<Value> = tags
                    .iter()
                    .enumerate()
                    .filter(|(_, e)| **e > 0)
                    .map(|(k, e)| {
                        let mut v = labels[k].clone();
                        v["power"] = json!(e);
                        v
                    })
                    .collect();
                // the text of the contribution, in the context without tags
                json!({ "factors": factors, "text": ctx.fmt_term(&single) })
            })
            .collect();
        Ok(json!({
            "word": self.word,
            "text": ctx.fmt_term(&crate::term::Term { terms: vec![(w.clone(), total)] }),
            "sources": sources,
        }))
    }
}

/// `workspace.trace`: see [`TraceWorkspace`].
pub fn trace_workspace(
    src: &str,
    presentation: Option<&str>,
    scalars: &[String],
    cell: &str,
    word: &str,
) -> Value {
    with_status((|| {
        let ws = Workspace::parse(src)?;
        let ring = ws.context.ring.clone();
        with_ring(
            &ring,
            TraceWorkspace {
                ws: &ws,
                presentation,
                scalars,
                cell,
                word,
            },
        )
    })())
}

fn cell_json(c: &Cell) -> Value {
    let kind = match &c.body {
        CellBody::Def { .. } => "def",
        CellBody::Watch { .. } => "watch",
        CellBody::Solve { .. } => "solve",
        CellBody::Check { .. } => "check",
        CellBody::Note { .. } => "note",
        CellBody::Comment(_) => "comment",
        CellBody::Blank => "blank",
    };
    let bare = Cell {
        id: None,
        options: vec![],
        ..c.clone()
    };
    let mut v = json!({
        "id": c.id,
        "kind": kind,
        "text": bare.print(),
        "options": c.options.iter().map(|(k, v)| (k.clone(), json!(v))).collect::<serde_json::Map<_, _>>(),
        "uses": c.names_used(),
    });
    match &c.body {
        CellBody::Def { name, term } => {
            v["defines"] = json!(name);
            v["term"] = json!(crate::text::print(term));
        }
        CellBody::Watch { term } => v["term"] = json!(crate::text::print(term)),
        CellBody::Solve { unknowns, .. } => v["unknowns"] = json!(unknowns),
        CellBody::Check { name, .. } => v["check"] = json!(name),
        CellBody::Note { text } => v["note"] = json!(text),
        _ => {}
    }
    v
}

/// `workspace.parse`: a workspace file, or one cell, in canonical form with what each cell
/// defines and mentions. A file's cells without an ID get one.
pub fn parse_workspace(workspace: Option<&str>, cell: Option<&str>) -> Value {
    with_status((|| match (workspace, cell) {
        (Some(src), None) => {
            let mut ws = Workspace::parse(src)?;
            ws.assign_ids();
            Ok(json!({
                "context": ws.context.print(),
                "cells": ws.cells.iter().map(cell_json).collect::<Vec<_>>(),
                "canonical": ws.print(),
            }))
        }
        (None, Some(src)) => Ok(json!({ "cell": cell_json(&Cell::parse(src)?) })),
        _ => Err(Error::invalid(
            "workspace.parse takes either 'workspace' or 'cell'",
        )),
    })())
}

struct Describe<'a> {
    spec: ContextSpec,
    presentation: Option<&'a str>,
}

impl RingVisitor for Describe<'_> {
    type Out = Value;
    fn visit<R: Ring>(self, base: R) -> Result<Value> {
        let pres = load_presentation(&self.spec, self.presentation)?;
        let ctx = Context::new(pres, self.spec, base)?;
        let s = Session::new(&ctx, None)?;
        Ok(json!({
            "context": ctx.spec.print(),
            "presentation": ctx.pres.name,
            "title": ctx.pres.title,
            "ring": ctx.spec.ring,
            "colours": ctx.colours.iter().map(|c| json!({ "name": c.name, "odd": c.odd, "oriented": c.oriented })).collect::<Vec<_>>(),
            "objects": ctx.objects.iter().map(|o| ctx.fmt_object(o)).collect::<Vec<_>>(),
            "generators": ctx.pres.generators.iter().map(|g| json!({
                "name": g.name,
                "written": g.written,
                "draw": g.draw,
                "inputs": g.source.len(),
                "outputs": g.target.len(),
                "source": crate::pres::print_pattern(&g.source, &g.vars),
                "target": crate::pres::print_pattern(&g.target, &g.vars),
            })).collect::<Vec<_>>(),
            "relations": ctx.pres.relations.iter().map(|r| r.name.clone()).collect::<Vec<_>>(),
            "representations": ctx.pres.representations.iter().map(|d| json!({
                "name": d.name,
                "kind": d.kind,
                "certificate": d.certificate_in(&ctx).status,
            })).collect::<Vec<_>>(),
            "scalars": ctx.spec.scalars,
            "provenance": provenance_json(s.eval.as_ref()),
        }))
    }
}

/// `context.describe`: checks a context line and says what it provides.
pub fn describe_context(context: &str, presentation: Option<&str>) -> Value {
    with_status((|| {
        let spec = ContextSpec::parse(context)?;
        let ring = spec.ring.clone();
        with_ring(&ring, Describe { spec, presentation })
    })())
}

/// The operations every surface exposes, by name.
pub const OPERATIONS: [&str; 17] = [
    "simplify",
    "check",
    "term.render",
    "presentation.check",
    "workspace.run",
    "workspace.trace",
    "workspace.parse",
    "context.describe",
    "relations.match",
    "relation.apply",
    "interchange",
    "slide",
    "straighten",
    "chain.find",
    "export.lean",
    "term.layout",
    "term.edit",
];

/// The JSON Schema of each operation (`spec/ops/`), and the shared definitions they refer to.
pub const SCHEMAS: [(&str, &str); 17] = [
    (
        "simplify",
        include_str!("../../../spec/ops/simplify.schema.json"),
    ),
    ("check", include_str!("../../../spec/ops/check.schema.json")),
    (
        "term.render",
        include_str!("../../../spec/ops/term.render.schema.json"),
    ),
    (
        "presentation.check",
        include_str!("../../../spec/ops/presentation.check.schema.json"),
    ),
    (
        "workspace.run",
        include_str!("../../../spec/ops/workspace.run.schema.json"),
    ),
    (
        "workspace.trace",
        include_str!("../../../spec/ops/workspace.trace.schema.json"),
    ),
    (
        "workspace.parse",
        include_str!("../../../spec/ops/workspace.parse.schema.json"),
    ),
    (
        "context.describe",
        include_str!("../../../spec/ops/context.describe.schema.json"),
    ),
    (
        "relations.match",
        include_str!("../../../spec/ops/relations.match.schema.json"),
    ),
    (
        "relation.apply",
        include_str!("../../../spec/ops/relation.apply.schema.json"),
    ),
    (
        "interchange",
        include_str!("../../../spec/ops/interchange.schema.json"),
    ),
    ("slide", include_str!("../../../spec/ops/slide.schema.json")),
    (
        "straighten",
        include_str!("../../../spec/ops/straighten.schema.json"),
    ),
    (
        "chain.find",
        include_str!("../../../spec/ops/chain.find.schema.json"),
    ),
    (
        "export.lean",
        include_str!("../../../spec/ops/export.lean.schema.json"),
    ),
    (
        "term.layout",
        include_str!("../../../spec/ops/term.layout.schema.json"),
    ),
    (
        "term.edit",
        include_str!("../../../spec/ops/term.edit.schema.json"),
    ),
];
pub const COMMON_SCHEMA: &str = include_str!("../../../spec/ops/common.schema.json");

/// Runs an operation from its JSON request (see `spec/ops/`), returning its JSON response.
/// The drawing orders of a canvas request: `orders` maps summands to orders for
/// `term.layout`; `order` is the edited summand's order for `term.edit`.
fn orders(input: &Value) -> Result<std::collections::BTreeMap<usize, Vec<usize>>> {
    let list = |v: &Value| -> Result<Vec<usize>> {
        v.as_array()
            .and_then(|a| a.iter().map(|x| x.as_u64().map(|n| n as usize)).collect())
            .ok_or_else(|| Error::invalid("an order is a list of layer numbers"))
    };
    let mut out = std::collections::BTreeMap::new();
    if let Some(m) = input.get("orders") {
        let m = m
            .as_object()
            .ok_or_else(|| Error::invalid("'orders' maps summand numbers to orders"))?;
        for (k, v) in m {
            let k: usize = k
                .parse()
                .map_err(|_| Error::invalid(format!("'{k}' is not a summand number")))?;
            out.insert(k, list(v)?);
        }
    }
    if let Some(v) = input.get("order") {
        let k = input.get("summand").and_then(Value::as_u64).unwrap_or(1) as usize;
        out.insert(k, list(v)?);
    }
    Ok(out)
}

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
                let shown =
                    match input.get("summands") {
                        None => None,
                        Some(v) => Some(v.as_u64().ok_or_else(|| {
                            Error::invalid("'summands' must be a nonnegative integer")
                        })? as usize),
                    };
                let scalars: Vec<String> = match input.get("scalars") {
                    None => vec![],
                    Some(v) => serde_json::from_value(v.clone())
                        .map_err(|_| Error::invalid("'scalars' must be a list of names"))?,
                };
                run_workspace_with(
                    &need("workspace")?,
                    str_field("presentation").as_deref(),
                    shown,
                    &scalars,
                )
            }
            "workspace.trace" => {
                let scalars: Vec<String> = match input.get("scalars") {
                    None => vec![],
                    Some(v) => serde_json::from_value(v.clone())
                        .map_err(|_| Error::invalid("'scalars' must be a list of names"))?,
                };
                trace_workspace(
                    &need("workspace")?,
                    str_field("presentation").as_deref(),
                    &scalars,
                    &need("cell")?,
                    &need("word")?,
                )
            }
            "workspace.parse" => parse_workspace(
                str_field("workspace").as_deref(),
                str_field("cell").as_deref(),
            ),
            "context.describe" => {
                describe_context(&need("context")?, str_field("presentation").as_deref())
            }
            "relations.match" | "relation.apply" | "interchange" | "slide" | "straighten"
            | "chain.find" | "export.lean" | "term.layout" | "term.edit" => {
                let uint = |k: &str, d: usize| -> Result<usize> {
                    match input.get(k) {
                        None => Ok(d),
                        Some(v) => v.as_u64().map(|n| n as usize).ok_or_else(|| {
                            Error::invalid(format!("'{k}' must be a nonnegative integer"))
                        }),
                    }
                };
                let pair = |v: &Value, k: &str| -> Result<(usize, usize)> {
                    let a = v.get(k).and_then(Value::as_array);
                    match a.map(|a| a.iter().map(Value::as_u64).collect::<Vec<_>>()) {
                        Some(x) if x.len() == 2 && x.iter().all(Option::is_some) => {
                            Ok((x[0].unwrap() as usize, x[1].unwrap() as usize))
                        }
                        _ => Err(Error::invalid(format!(
                            "rectangle.{k} must be two numbers, first and last"
                        ))),
                    }
                };
                let rectangle = match input.get("rectangle") {
                    None => None,
                    Some(r) => Some((pair(r, "layers")?, pair(r, "strands")?)),
                };
                let r = crate::ops_rewrite::RewriteRequest {
                    req: req()?,
                    term: need("term")?,
                    rhs: str_field("rhs"),
                    summand: uint("summand", 1)?,
                    rectangle,
                    relation: str_field("relation"),
                    index: uint("match", 0)?,
                    everywhere: input.get("everywhere").and_then(Value::as_bool) == Some(true),
                    layer: uint("layer", 0)?,
                    layers: match input.get("layers") {
                        None => Vec::new(),
                        Some(v) => v
                            .as_array()
                            .and_then(|a| {
                                a.iter().map(|x| x.as_u64().map(|n| n as usize)).collect()
                            })
                            .ok_or_else(|| Error::invalid("'layers' is a list of layer numbers"))?,
                    },
                    direction: str_field("direction").unwrap_or_default(),
                    steps: uint("steps", 0)?,
                    orders: orders(input)?,
                    shown: uint("summands", 0)?,
                    action: str_field("action").unwrap_or_default(),
                    generator: str_field("generator").unwrap_or_default(),
                    below: uint("below", 0)?,
                    position: uint("position", 0)?,
                    max_steps: uint("max_steps", 0)?,
                    name: str_field("name").unwrap_or_default(),
                    doc: str_field("doc").unwrap_or_default(),
                };
                crate::ops_rewrite::run(op, &r)
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
    fn quotients_and_finite_checks() {
        let r = req("klr type=A2 nu=2i+j Lambda=2i ring=QQ");
        let v = simplify(&r, "x_1^2 e(iij) + x_1 e(iji)");
        assert_eq!(v["status"], "ok", "{v}");
        assert_eq!(v["normal_form"], "x_1 e(iji)", "{v}");
        assert_eq!(
            v["provenance"]["strategy"],
            "finite-dimensional linear algebra"
        );
        assert!(
            v["provenance"]["notes"][0]
                .as_str()
                .unwrap()
                .contains("cyclotomic quotient")
        );
        let one = "e(iij) + e(iji) + e(jii)".to_string();
        let v = check(&r, "central", std::slice::from_ref(&one));
        assert_eq!(v["holds"], true, "{v}");
        let v = check(&r, "central", &["x_2 e(iij)".into()]);
        assert_eq!(v["holds"], false, "{v}");
        let v = check(&r, "primitive", &["e(iji)".into()]);
        assert_eq!(v["holds"], true, "{v}");
        let v = check(&r, "primitive", &["e(iij)".into()]);
        assert_eq!(v["holds"], false, "{v}");
        // a truncation states that it is one, and has no radical to test primitivity with
        let r = req("klr type=A2 nu=2i+j degcap=2 ring=QQ");
        let v = simplify(&r, "x_1 e(iij)");
        assert!(
            v["provenance"]["notes"].to_string().contains("truncated"),
            "{v}"
        );
        let v = check(&r, "primitive", &["e(iji)".into()]);
        assert_eq!(v["status"], "error", "{v}");
        // central works in the algebra itself too
        let v = check(
            &req("nilhecke nu=2 ring=QQ"),
            "central",
            &["x_1 + x_2".into()],
        );
        assert_eq!(v["holds"], true, "{v}");
    }

    #[test]
    fn solving() {
        let src = "diagcalc-workspace/1
context klr type=A2 nu=2i+j ring=QQ
def e := c1 x_1 psi_1 e(iij) + c2 psi_1 x_2 e(iij)  # id=d1
solve e = e(iij) for c1, c2  # id=s2
solve e e - e = 0 for c1, c2  # id=s3
solve c3 e(iij) = 0 for c1, c2  # id=s4
";
        let v = run_workspace(src, None);
        assert_eq!(v["status"], "ok", "{v}");
        let cells = v["cells"].as_array().unwrap();
        // x_1 psi_1 = psi_1 x_2 + 1 on e(ii): linear, one solution
        assert_eq!(cells[1]["tier"], 1);
        assert_eq!(cells[1]["outcome"], "unique");
        assert_eq!(cells[1]["solution"], json!({"c1": "1", "c2": "-1"}));
        // e e = e: quadratic; four idempotents 0, x_1 psi_1, -psi_1 x_2, 1 on e(iij)
        assert_eq!(cells[2]["tier"], 2);
        assert_eq!(cells[2]["outcome"], "finite");
        assert_eq!(cells[2]["distinct_solutions"], 4);
        assert_eq!(cells[2]["all_found"], true);
        assert_eq!(cells[2]["points"].as_array().unwrap().len(), 4);
        // c3 is neither an unknown nor a scalar
        assert_eq!(cells[3]["status"], "error");
        // with no equation at all, every value solves
        let src = "diagcalc-workspace/1
context nilhecke nu=2 ring=QQ
solve c1 x_1 - c1 x_1 = 0 for c1
";
        let v = run_workspace(src, None);
        assert_eq!(v["cells"][0]["outcome"], "family", "{v}");
        assert_eq!(v["cells"][0]["dimension"], 1, "{v}");
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
        // a is not zero, whatever c is
        assert_eq!(cells[4]["outcome"], "none", "{v}");
        assert!(cells.iter().all(|c| c.get("warnings").is_none()), "{v}");
    }

    #[test]
    fn tracing_a_summand() {
        let src = "diagcalc-workspace/1
context nilhecke nu=2 ring=QQ
def a := x_1 + psi_1  # id=d1
watch a a + 3 x_1 psi_1  # id=w2
";
        let v = trace_workspace(src, None, &[], "w2", "psi_1 x_2");
        assert_eq!(v["status"], "ok", "{v}");
        assert_eq!(v["text"], "4 psi_1 x_2", "{v}");
        let sources = v["sources"].as_array().unwrap();
        assert_eq!(sources.len(), 2, "{v}");
        // x_1 psi_1 = psi_1 x_2 + 1: from a's two terms, and from the watch's own term
        let from_a = &sources[0];
        assert_eq!(from_a["text"], "psi_1 x_2", "{v}");
        let f = from_a["factors"].as_array().unwrap();
        assert_eq!(f.len(), 2, "{v}");
        assert!(
            f.iter()
                .all(|x| x["name"] == "a" && x["cell"] == "d1" && x["power"] == 1)
        );
        assert_eq!(sources[1]["factors"], json!([]), "{v}");
        assert_eq!(sources[1]["text"], "3 psi_1 x_2", "{v}");
        // a square uses one term twice
        let v = trace_workspace(src, None, &[], "w2", "x_1^2");
        assert_eq!(v["sources"][0]["factors"][0]["power"], 2, "{v}");
        let v = trace_workspace(src, None, &[], "d1", "x_1");
        assert_eq!(v["status"], "error", "{v}");
    }

    #[test]
    fn idempotents_outside_the_weight_are_named() {
        let src = "diagcalc-workspace/1
context klr type=A2 nu=2i+j ring=QQ
watch e(ii) + x_1 e(iji)  # id=w1
def a := psi_1 e(iij)  # id=d2
check zero(e(jj) e(jj))  # id=k3
";
        let v = run_workspace(src, None);
        assert_eq!(v["status"], "ok", "{v}");
        let cells = v["cells"].as_array().unwrap();
        assert_eq!(cells[0]["normal_form"], "x_1 e(iji)", "{v}");
        assert_eq!(
            cells[0]["warnings"],
            json!(["$e(ii)$ is $0$ here: its object is not one of weight $\\nu = 2i+j$"]),
            "{v}"
        );
        assert!(cells[1].get("warnings").is_none(), "{v}");
        assert_eq!(cells[2]["holds"], true, "{v}");
        assert_eq!(cells[2]["warnings"].as_array().unwrap().len(), 1, "{v}");
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
