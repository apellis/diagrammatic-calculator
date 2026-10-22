//! Presentations of strict monoidal supercategories, and the presentation file format
//! (`.pres.toml`, format `diagcalc-presentation/1`).
//!
//! A presentation fixes colours, generators (possibly schemas over colour variables), parameter
//! polynomials, relation schemas with side conditions, and the representations that can evaluate
//! its terms. Nothing here is specific to any family of algebras; see `docs/design.md`.

use crate::error::{Error, Result};
use crate::expr::{Cond, IntExpr, parse_cond, parse_int_expr};
use serde::Deserialize;
use std::collections::BTreeMap;

pub const PRESENTATION_FORMAT: &str = "diagcalc-presentation/1";

/// A colour: a strand label with a parity and an orientation mode.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Colour {
    pub name: String,
    pub odd: bool,
    /// Oriented strands carry arrows and come in up (`i`) and down (`i*`) versions.
    pub oriented: bool,
}

/// Where the colours of a presentation come from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ColourSpec {
    /// Fixed in the file.
    Fixed(Vec<Colour>),
    /// One colour per simple root of the context's Cartan datum (KLR algebras and relatives).
    Cartan,
}

/// Which objects a word without an idempotent ranges over.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ObjectSpec {
    /// All sequences of a weight `ν` (upward strands), given by the context.
    Weight,
    /// Only objects written explicitly.
    Explicit,
}

/// A colour in a generator's source or target pattern: a colour variable or a fixed colour.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PatColour {
    Var(usize),
    Fixed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PatLetter {
    pub colour: PatColour,
    /// Upward (`a`) or downward (`a*`).
    pub up: bool,
}

/// A generator, possibly a schema over colour variables.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Generator {
    pub name: String,
    pub aliases: Vec<String>,
    pub vars: Vec<String>,
    pub source: Vec<PatLetter>,
    pub target: Vec<PatLetter>,
    pub degree: IntExpr,
    pub parity: IntExpr,
    /// How the drawing code shows it: `dot`, `crossing`, `cup`, `cap`, `box`.
    pub draw: String,
}

/// A parameter family indexed by pairs of distinct colours, such as `Q_ab(u, v)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Parameter {
    pub name: String,
    pub vars: Vec<String>,
    /// A built-in default the context may override (`klq2`), or a polynomial in `vars`.
    pub default: String,
    /// Stated hypotheses, checked by the conformance checker where it can.
    pub hypotheses: Vec<String>,
    /// If set, this parameter is derived from another: `qbar(Q)`.
    pub derived: Option<String>,
}

/// A relation schema: `lhs = rhs` for every binding of the colour variables satisfying `when`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Relation {
    pub name: String,
    pub vars: Vec<String>,
    pub when: Cond,
    pub lhs: String,
    pub rhs: String,
}

/// A faithfulness certificate as stated in a file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Certificate {
    /// `lean`, `literature`, `assumed` or `unknown`.
    pub status: String,
    pub reference: String,
    pub hypotheses: String,
}

/// A representation, by built-in kind, with the roles of the generators.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepresentationDecl {
    pub name: String,
    /// Built-in action kind: `klr-polynomial`, `nil-coxeter`, `odd-nil-coxeter`.
    pub kind: String,
    /// Generator names by role (`dot`, `crossing`).
    pub roles: BTreeMap<String, String>,
    pub certificate: Certificate,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Presentation {
    pub name: String,
    pub title: String,
    pub colours: ColourSpec,
    pub objects: ObjectSpec,
    pub generators: Vec<Generator>,
    pub parameters: Vec<Parameter>,
    pub relations: Vec<Relation>,
    pub representations: Vec<RepresentationDecl>,
    /// How the presentation is written in a Lean library, for exporting rewrite chains.
    pub lean: Option<LeanDecl>,
}

/// The Lean counterpart of a presentation: where its diagrams live and which lemma proves each
/// relation. Templates write `{a}` for a colour variable and `{a!=b}` for a proof that two
/// colours differ.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LeanDecl {
    /// The encoding of diagrams the exporter writes (`kl3-dg`).
    pub encoding: String,
    /// The modules the exported file imports, separated by spaces.
    pub module: String,
    /// The namespace the exported theorems are stated in, and the namespaces it opens.
    pub namespace: String,
    pub open: String,
    /// Each generator's Lean shape.
    pub generators: BTreeMap<String, String>,
    /// For each relation that has one, the Lean lemma proving it.
    pub relations: BTreeMap<String, LeanRelation>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LeanRelation {
    /// The lemma, applied at the weight `ν`.
    pub lemma: String,
    /// Cartan data the lemma's statement depends on, such as `dot {a} {b}` or `dij {a} {b}`:
    /// the exported theorem takes their values as hypotheses. `dot≠0 {a} {b}` is a hypothesis
    /// of the lemma: the relation exports only where the pairing is nonzero.
    pub facts: Vec<String>,
    /// How the lemma lays out the words of its right-hand side: `canonical` (as the calculator
    /// does) or `left-first` (generators on the left strands lower).
    pub layering: String,
}

impl Presentation {
    pub fn generator(&self, name: &str) -> Option<usize> {
        self.generators
            .iter()
            .position(|g| g.name == name || g.aliases.iter().any(|a| a == name))
    }
    pub fn parameter(&self, name: &str) -> Option<&Parameter> {
        self.parameters.iter().find(|p| p.name == name)
    }
}

// ---------------------------------------------------------------------------------------------
// Reading the TOML file.

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPres {
    format: String,
    name: String,
    #[serde(default)]
    title: String,
    colours: toml::Value,
    #[serde(default = "default_objects")]
    objects: String,
    #[serde(default)]
    parameters: BTreeMap<String, RawParam>,
    generators: BTreeMap<String, RawGen>,
    #[serde(default)]
    relations: Vec<RawRel>,
    #[serde(default)]
    representations: Vec<RawRep>,
    /// Order of generators in the file (TOML tables are unordered).
    #[serde(default)]
    generator_order: Vec<String>,
    #[serde(default)]
    lean: Option<RawLean>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLean {
    encoding: String,
    module: String,
    namespace: String,
    #[serde(default)]
    open: String,
    generators: BTreeMap<String, String>,
    #[serde(default)]
    relations: BTreeMap<String, RawLeanRel>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLeanRel {
    lemma: String,
    #[serde(default)]
    facts: Vec<String>,
    #[serde(default = "canonical_layering")]
    layering: String,
}

fn canonical_layering() -> String {
    "canonical".into()
}

fn default_objects() -> String {
    "explicit".into()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawColour {
    name: String,
    #[serde(default = "even")]
    parity: String,
    #[serde(default)]
    oriented: bool,
}

fn even() -> String {
    "even".into()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawGen {
    #[serde(default)]
    colours: String,
    source: String,
    target: String,
    degree: toml::Value,
    #[serde(default = "zero")]
    parity: toml::Value,
    #[serde(default = "box_draw")]
    draw: String,
    #[serde(default)]
    aliases: Vec<String>,
}

fn zero() -> toml::Value {
    toml::Value::Integer(0)
}
fn box_draw() -> String {
    "box".into()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawParam {
    vars: Vec<String>,
    #[serde(default)]
    default: String,
    #[serde(default)]
    hypotheses: Vec<String>,
    #[serde(default)]
    derived: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRel {
    name: String,
    #[serde(default)]
    colours: String,
    #[serde(default)]
    when: Option<String>,
    lhs: String,
    rhs: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRep {
    name: String,
    kind: String,
    roles: BTreeMap<String, String>,
    certificate: RawCert,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCert {
    status: String,
    #[serde(default)]
    reference: String,
    #[serde(default)]
    hypotheses: String,
}

fn words(s: &str) -> Vec<String> {
    s.split_whitespace().map(String::from).collect()
}

fn int_or_expr(v: &toml::Value, vars: &[String], what: &str) -> Result<IntExpr> {
    match v {
        toml::Value::Integer(n) => Ok(IntExpr::Const(*n)),
        toml::Value::String(s) => {
            parse_int_expr(s, vars).map_err(|e| Error::invalid(format!("{what}: {}", e.message)))
        }
        _ => Err(Error::invalid(format!(
            "{what} must be an integer or an expression"
        ))),
    }
}

fn pattern(s: &str, vars: &[String]) -> Vec<PatLetter> {
    s.split_whitespace()
        .map(|w| {
            let (name, up) = match w.strip_suffix('*') {
                Some(n) => (n, false),
                None => (w, true),
            };
            let colour = match vars.iter().position(|v| v == name) {
                Some(i) => PatColour::Var(i),
                None => PatColour::Fixed(name.to_string()),
            };
            PatLetter { colour, up }
        })
        .collect()
}

fn print_pattern(p: &[PatLetter], vars: &[String]) -> String {
    p.iter()
        .map(|l| {
            let n = match &l.colour {
                PatColour::Var(i) => vars[*i].clone(),
                PatColour::Fixed(s) => s.clone(),
            };
            if l.up { n } else { format!("{n}*") }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

impl Presentation {
    /// Reads a presentation file.
    pub fn from_toml(text: &str) -> Result<Presentation> {
        let raw: RawPres =
            toml::from_str(text).map_err(|e| Error::invalid(format!("presentation file: {e}")))?;
        if raw.format != PRESENTATION_FORMAT {
            return Err(Error::invalid(format!(
                "unsupported format '{}' (this version reads {PRESENTATION_FORMAT})",
                raw.format
            )));
        }
        let colours = match &raw.colours {
            toml::Value::String(s) if s == "cartan" => ColourSpec::Cartan,
            toml::Value::Array(_) => {
                let list: Vec<RawColour> = raw
                    .colours
                    .clone()
                    .try_into()
                    .map_err(|e| Error::invalid(format!("colours: {e}")))?;
                let mut cs = Vec::new();
                for c in list {
                    let odd = match c.parity.as_str() {
                        "even" => false,
                        "odd" => true,
                        p => return Err(Error::invalid(format!("colour parity '{p}'"))),
                    };
                    cs.push(Colour {
                        name: c.name,
                        odd,
                        oriented: c.oriented,
                    });
                }
                ColourSpec::Fixed(cs)
            }
            _ => return Err(Error::invalid("colours must be \"cartan\" or a list")),
        };
        let objects = match raw.objects.as_str() {
            "weight" => ObjectSpec::Weight,
            "explicit" => ObjectSpec::Explicit,
            o => return Err(Error::invalid(format!("objects = '{o}'"))),
        };
        let mut order = raw.generator_order.clone();
        for k in raw.generators.keys() {
            if !order.contains(k) {
                order.push(k.clone());
            }
        }
        let mut generators = Vec::new();
        for name in &order {
            let g = raw
                .generators
                .get(name)
                .ok_or_else(|| Error::invalid(format!("generator_order names unknown '{name}'")))?;
            let vars = words(&g.colours);
            generators.push(Generator {
                name: name.clone(),
                aliases: g.aliases.clone(),
                source: pattern(&g.source, &vars),
                target: pattern(&g.target, &vars),
                degree: int_or_expr(&g.degree, &vars, &format!("degree of {name}"))?,
                parity: int_or_expr(&g.parity, &vars, &format!("parity of {name}"))?,
                draw: g.draw.clone(),
                vars,
            });
        }
        let parameters = raw
            .parameters
            .into_iter()
            .map(|(name, p)| Parameter {
                name,
                vars: p.vars,
                default: p.default,
                hypotheses: p.hypotheses,
                derived: p.derived,
            })
            .collect();
        let mut relations = Vec::new();
        for r in raw.relations {
            let vars = words(&r.colours);
            let when = match &r.when {
                Some(w) => parse_cond(w, &vars)
                    .map_err(|e| Error::invalid(format!("relation {}: {}", r.name, e.message)))?,
                None => Cond::True,
            };
            relations.push(Relation {
                name: r.name,
                vars,
                when,
                lhs: r.lhs,
                rhs: r.rhs,
            });
        }
        let representations = raw
            .representations
            .into_iter()
            .map(|r| RepresentationDecl {
                name: r.name,
                kind: r.kind,
                roles: r.roles,
                certificate: Certificate {
                    status: r.certificate.status,
                    reference: r.certificate.reference,
                    hypotheses: r.certificate.hypotheses,
                },
            })
            .collect();
        let lean = match raw.lean {
            None => None,
            Some(l) => {
                for g in l.generators.keys() {
                    if !generators.iter().any(|x| &x.name == g) {
                        return Err(Error::invalid(format!("lean: no generator '{g}'")));
                    }
                }
                for (r, x) in &l.relations {
                    if !relations.iter().any(|y| &y.name == r) {
                        return Err(Error::invalid(format!("lean: no relation '{r}'")));
                    }
                    if !["canonical", "left-first"].contains(&x.layering.as_str()) {
                        return Err(Error::invalid(format!(
                            "lean: relation {r} has unknown layering '{}'",
                            x.layering
                        )));
                    }
                }
                Some(LeanDecl {
                    encoding: l.encoding,
                    module: l.module,
                    namespace: l.namespace,
                    open: l.open,
                    generators: l.generators,
                    relations: l
                        .relations
                        .into_iter()
                        .map(|(k, r)| {
                            (
                                k,
                                LeanRelation {
                                    lemma: r.lemma,
                                    facts: r.facts,
                                    layering: r.layering,
                                },
                            )
                        })
                        .collect(),
                })
            }
        };
        Ok(Presentation {
            name: raw.name,
            title: raw.title,
            colours,
            objects,
            generators,
            parameters,
            relations,
            representations,
            lean,
        })
    }

    /// Writes the presentation in canonical form; `from_toml` of the result gives it back.
    pub fn to_toml(&self) -> String {
        let q = |s: &str| toml::Value::String(s.to_string()).to_string();
        let mut o = String::new();
        o.push_str(&format!("format = {}\n", q(PRESENTATION_FORMAT)));
        o.push_str(&format!("name = {}\n", q(&self.name)));
        if !self.title.is_empty() {
            o.push_str(&format!("title = {}\n", q(&self.title)));
        }
        match &self.colours {
            ColourSpec::Cartan => o.push_str("colours = \"cartan\"\n"),
            ColourSpec::Fixed(cs) => {
                o.push_str("colours = [\n");
                for c in cs {
                    o.push_str(&format!(
                        "  {{ name = {}, parity = \"{}\", oriented = {} }},\n",
                        q(&c.name),
                        if c.odd { "odd" } else { "even" },
                        c.oriented
                    ));
                }
                o.push_str("]\n");
            }
        }
        o.push_str(&format!(
            "objects = \"{}\"\n",
            match self.objects {
                ObjectSpec::Weight => "weight",
                ObjectSpec::Explicit => "explicit",
            }
        ));
        let names: Vec<String> = self.generators.iter().map(|g| q(&g.name)).collect();
        o.push_str(&format!("generator_order = [{}]\n", names.join(", ")));
        let mut params: Vec<&Parameter> = self.parameters.iter().collect();
        params.sort_by(|a, b| a.name.cmp(&b.name));
        for p in params {
            o.push_str(&format!("\n[parameters.{}]\n", p.name));
            let vs: Vec<String> = p.vars.iter().map(|v| q(v)).collect();
            o.push_str(&format!("vars = [{}]\n", vs.join(", ")));
            if !p.default.is_empty() {
                o.push_str(&format!("default = {}\n", q(&p.default)));
            }
            if let Some(d) = &p.derived {
                o.push_str(&format!("derived = {}\n", q(d)));
            }
            if !p.hypotheses.is_empty() {
                let hs: Vec<String> = p.hypotheses.iter().map(|h| q(h)).collect();
                o.push_str(&format!("hypotheses = [{}]\n", hs.join(", ")));
            }
        }
        for g in &self.generators {
            o.push_str(&format!("\n[generators.{}]\n", g.name));
            if !g.vars.is_empty() {
                o.push_str(&format!("colours = {}\n", q(&g.vars.join(" "))));
            }
            o.push_str(&format!(
                "source = {}\n",
                q(&print_pattern(&g.source, &g.vars))
            ));
            o.push_str(&format!(
                "target = {}\n",
                q(&print_pattern(&g.target, &g.vars))
            ));
            let ie = |e: &IntExpr| match e {
                IntExpr::Const(n) => n.to_string(),
                e => q(&e.print(&g.vars)),
            };
            o.push_str(&format!("degree = {}\n", ie(&g.degree)));
            o.push_str(&format!("parity = {}\n", ie(&g.parity)));
            o.push_str(&format!("draw = {}\n", q(&g.draw)));
            if !g.aliases.is_empty() {
                let al: Vec<String> = g.aliases.iter().map(|a| q(a)).collect();
                o.push_str(&format!("aliases = [{}]\n", al.join(", ")));
            }
        }
        for r in &self.relations {
            o.push_str("\n[[relations]]\n");
            o.push_str(&format!("name = {}\n", q(&r.name)));
            if !r.vars.is_empty() {
                o.push_str(&format!("colours = {}\n", q(&r.vars.join(" "))));
            }
            if r.when != Cond::True {
                o.push_str(&format!("when = {}\n", q(&r.when.print(&r.vars))));
            }
            o.push_str(&format!("lhs = {}\n", q(&r.lhs)));
            o.push_str(&format!("rhs = {}\n", q(&r.rhs)));
        }
        for r in &self.representations {
            o.push_str("\n[[representations]]\n");
            o.push_str(&format!("name = {}\n", q(&r.name)));
            o.push_str(&format!("kind = {}\n", q(&r.kind)));
            let roles: Vec<String> = r
                .roles
                .iter()
                .map(|(k, v)| format!("{k} = {}", q(v)))
                .collect();
            o.push_str(&format!("roles = {{ {} }}\n", roles.join(", ")));
            let c = &r.certificate;
            o.push_str(&format!("certificate.status = {}\n", q(&c.status)));
            if !c.reference.is_empty() {
                o.push_str(&format!("certificate.reference = {}\n", q(&c.reference)));
            }
            if !c.hypotheses.is_empty() {
                o.push_str(&format!("certificate.hypotheses = {}\n", q(&c.hypotheses)));
            }
        }
        if let Some(l) = &self.lean {
            o.push_str("\n[lean]\n");
            o.push_str(&format!("encoding = {}\n", q(&l.encoding)));
            o.push_str(&format!("module = {}\n", q(&l.module)));
            o.push_str(&format!("namespace = {}\n", q(&l.namespace)));
            if !l.open.is_empty() {
                o.push_str(&format!("open = {}\n", q(&l.open)));
            }
            let gs: Vec<String> = l
                .generators
                .iter()
                .map(|(k, v)| format!("{k} = {}", q(v)))
                .collect();
            o.push_str(&format!("generators = {{ {} }}\n", gs.join(", ")));
            for (name, r) in &l.relations {
                o.push_str(&format!("\n[lean.relations.{name}]\n"));
                o.push_str(&format!("lemma = {}\n", q(&r.lemma)));
                if !r.facts.is_empty() {
                    let fs: Vec<String> = r.facts.iter().map(|f| q(f)).collect();
                    o.push_str(&format!("facts = [{}]\n", fs.join(", ")));
                }
                if r.layering != "canonical" {
                    o.push_str(&format!("layering = {}\n", q(&r.layering)));
                }
            }
        }
        o
    }
}
