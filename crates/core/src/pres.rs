//! Presentations of strict monoidal supercategories, and the presentation file format
//! (`.pres.toml`, format `diagcalc-presentation/1`).
//!
//! A presentation fixes colours, generators (possibly schemas over colour variables), parameter
//! polynomials, relation schemas with side conditions, and the representations that can evaluate
//! its terms. Nothing here is specific to any family of algebras; see `docs/design.md`.

use crate::error::{Error, Result};
use crate::expr::{Cond, IntExpr, TermInt, parse_cond, parse_int_expr, parse_term_int};
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
    /// The name typed and printed. Several generators may share one, told apart by the
    /// strands they sit on (a dot on an upward or a downward strand) or, for a generator with
    /// no source such as a cup, by the colours written after its position (`cup_{1,i*}`).
    pub written: String,
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
    /// Integer variables with their ranges `lo..hi` (both ends included), which may use `h(a)`:
    /// one instance per value, as for the colour variables.
    pub ints: Vec<(String, TermInt, TermInt)>,
    pub when: Cond,
    pub lhs: String,
    pub rhs: String,
}

/// A faithfulness certificate as stated in a file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Certificate {
    /// `lean`, `literature`, `conditional` (proved under a named open conjecture: equalities it
    /// finds are proven, differences rest on the conjecture), `assumed` or `unknown`.
    pub status: String,
    pub reference: String,
    pub hypotheses: String,
}

/// A representation, by built-in kind, with the roles of the generators.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepresentationDecl {
    pub name: String,
    /// Built-in action kind: `klr-polynomial`, `nil-coxeter`, `odd-nil-coxeter`, `kl-basis`,
    /// `skm-basis`, `quiver-hecke-basis`, `translation`.
    pub kind: String,
    /// Generator names by role (`dot`, `crossing`).
    pub roles: BTreeMap<String, String>,
    /// For kind `translation`: the shipped presentation whose first representation decides
    /// equality, and each generator's image there, a term in the generator's colour variables.
    /// Every generator of the target's normal forms must be the image of exactly one generator.
    pub target: String,
    pub images: BTreeMap<String, String>,
    /// The representation holds only when every colour is even (a translation into an even
    /// presentation, an even evaluation); elsewhere it does not apply.
    pub even_only: bool,
    pub certificate: Certificate,
    /// The certificate in a context whose colours are all even, when it is stronger there (the
    /// conjecture behind `certificate` is a theorem for even data).
    pub even_certificate: Option<Certificate>,
}

impl RepresentationDecl {
    /// The certificate that holds in a context: `even_certificate` when every colour is even and
    /// one is declared, `certificate` otherwise.
    pub fn certificate_in<R: crate::ring::Ring>(
        &self,
        ctx: &crate::context::Context<R>,
    ) -> &Certificate {
        match &self.even_certificate {
            Some(c) if ctx.colours.iter().all(|col| !col.odd) => c,
            _ => &self.certificate,
        }
    }
}

/// A family of bubbles, written `name_{a,m}` for a colour `a` and an integer label `m`: for
/// `m ≥ 0` the diagram `word` (a term in the colour variable and `m`), for `m < 0` the fake
/// bubble defined by the infinite Grassmannian relation with the partner family, or zero.
///
/// `index` is the bubble's degree index, linear in `m` with slope 1 (degree `2 d_a` times it;
/// `m + 1 - h(a)` for the clockwise KL bubble). A fake bubble of index `k ≥ 0` is
/// `f_k` with `f_0 = 1` and `f_k = -Σ_{j=1}^k p_j f_{k-j}`, where `p_j` is the partner's real
/// bubble of index `j` (zero when its label is negative); a fake bubble of negative index is
/// zero.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BubbleFamily {
    pub name: String,
    pub var: String,
    pub word: String,
    pub index: TermInt,
    pub partner: String,
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
    pub bubbles: Vec<BubbleFamily>,
    pub representations: Vec<RepresentationDecl>,
    /// How the presentation is written in a Lean library, for exporting rewrite chains.
    pub lean: Option<LeanDecl>,
}

/// The Lean counterpart of a presentation: where its diagrams live and which lemma proves each
/// relation. Templates write `{a}` for a colour variable and `{a!=b}` for a proof that two
/// colours differ.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LeanDecl {
    /// The encoding of diagrams the exporter writes (`kl3-dg` or `skm-cl`).
    pub encoding: String,
    /// The modules the exported file imports, separated by spaces.
    pub module: String,
    /// The namespace the exported theorems are stated in, and the namespaces it opens.
    pub namespace: String,
    pub open: String,
    /// Each generator's Lean shape.
    pub generators: BTreeMap<String, String>,
    /// Generators that are not Lean generators but are defined from them (BE2's leftward cups
    /// and caps, downward dots and crossings): for each, the cases of its expansion into layers.
    pub derived: BTreeMap<String, Vec<LeanDerived>>,
    /// For each relation that has one, the Lean lemma proving it.
    pub relations: BTreeMap<String, LeanRelation>,
}

/// One case of a derived generator's expansion: where `when` holds (evaluated, like a degree, at
/// the region just right of the generator), the generator is `(−1)^sign` (negated if `negate`)
/// times its layers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LeanDerived {
    pub when: String,
    pub sign: String,
    pub negate: bool,
    /// Each layer: the letters left of the shape and right of it inside the generator's strands
    /// (Lean letters such as `dn {a}`, separated by commas), the shape, and how many times the
    /// layer repeats (an integer expression; empty for once). `{a}` names a colour variable, any
    /// other `{…}` an integer expression.
    pub layers: Vec<[String; 4]>,
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
    /// does), `left-first` (generators on the left strands lower), `right-first` (on the right
    /// strands lower) or `bubbles-first` (closed components, such as bubbles, lower).
    pub layering: String,
    /// How the lemma lays out the words of its left-hand side, with the same values as
    /// `layering`; the exported proof reaches it from the calculator's layout by interchange.
    pub pattern_layering: String,
    /// How the lemma proves the relation as the calculator states it: `rw` (rewrite with it,
    /// then normalize the sums), `exact` (it is the statement, up to unfolding) or `tactic`
    /// (`lemma` is a tactic script, its steps separated by `; `).
    pub proof: String,
}

impl Presentation {
    pub fn generator(&self, name: &str) -> Option<usize> {
        self.generators
            .iter()
            .position(|g| g.name == name || g.aliases.iter().any(|a| a == name))
    }
    /// The generators a typed name may mean: those written that way, else the one so named.
    pub fn generators_written(&self, name: &str) -> Vec<usize> {
        let w: Vec<usize> = (0..self.generators.len())
            .filter(|&i| {
                let g = &self.generators[i];
                g.written == name || g.aliases.iter().any(|a| a == name)
            })
            .collect();
        if w.is_empty() {
            self.generator(name).into_iter().collect()
        } else {
            w
        }
    }
    /// Whether a relation's instances depend on the weight of the region right of them: its
    /// side condition, integer ranges or terms mention `h(...)`, or its terms use bubbles
    /// (whose fake values depend on the region).
    pub fn relation_uses_region(&self, r: &Relation) -> bool {
        let text = format!("{} {}", r.lhs, r.rhs);
        r.when.uses_pairing()
            || !r.ints.is_empty()
            || text.contains("h(")
            || self
                .bubbles
                .iter()
                .any(|b| text.contains(&format!("{}_", b.name)))
    }
    pub fn bubble(&self, name: &str) -> Option<&BubbleFamily> {
        self.bubbles.iter().find(|b| b.name == name)
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
    bubbles: BTreeMap<String, RawBubble>,
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
    derived: BTreeMap<String, Vec<RawLeanDerived>>,
    #[serde(default)]
    relations: BTreeMap<String, RawLeanRel>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLeanDerived {
    #[serde(default)]
    when: String,
    #[serde(default)]
    sign: String,
    #[serde(default)]
    negate: bool,
    layers: Vec<Vec<String>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLeanRel {
    lemma: String,
    #[serde(default)]
    facts: Vec<String>,
    #[serde(default = "canonical_layering")]
    layering: String,
    #[serde(default = "canonical_layering")]
    pattern_layering: String,
    #[serde(default = "rw_proof")]
    proof: String,
}

fn rw_proof() -> String {
    "rw".into()
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
    #[serde(default)]
    written: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawBubble {
    colours: String,
    word: String,
    index: String,
    partner: String,
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
    #[serde(default)]
    ints: BTreeMap<String, String>,
    lhs: String,
    rhs: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRep {
    name: String,
    kind: String,
    #[serde(default)]
    roles: BTreeMap<String, String>,
    #[serde(default)]
    target: String,
    #[serde(default)]
    images: BTreeMap<String, String>,
    #[serde(default)]
    even_only: bool,
    certificate: RawCert,
    #[serde(default)]
    even_certificate: Option<RawCert>,
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

impl From<RawCert> for Certificate {
    fn from(c: RawCert) -> Self {
        Certificate {
            status: c.status,
            reference: c.reference,
            hypotheses: c.hypotheses,
        }
    }
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

/// A source or target pattern as written in the presentation file: `a* b`.
pub fn print_pattern(p: &[PatLetter], vars: &[String]) -> String {
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
                written: g.written.clone().unwrap_or_else(|| name.clone()),
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
            let mut ints = Vec::new();
            for (v, range) in &r.ints {
                let bad = || {
                    Error::invalid(format!(
                        "relation {}: the range of {v} is written lo..hi",
                        r.name
                    ))
                };
                let (lo, hi) = range.split_once("..").ok_or_else(bad)?;
                let parse = |t: &str| {
                    parse_term_int(t)
                        .map_err(|e| Error::invalid(format!("relation {}: {}", r.name, e.message)))
                };
                ints.push((v.clone(), parse(lo)?, parse(hi)?));
            }
            relations.push(Relation {
                name: r.name,
                vars,
                ints,
                when,
                lhs: r.lhs,
                rhs: r.rhs,
            });
        }
        let mut bubbles = Vec::new();
        for (name, b) in raw.bubbles {
            let vars = words(&b.colours);
            if vars.len() != 1 {
                return Err(Error::invalid(format!(
                    "bubbles.{name}: one colour variable"
                )));
            }
            bubbles.push(BubbleFamily {
                index: parse_term_int(&b.index)
                    .map_err(|e| Error::invalid(format!("bubbles.{name}: {}", e.message)))?,
                name,
                var: vars[0].clone(),
                word: b.word,
                partner: b.partner,
            });
        }
        for b in &bubbles {
            if !bubbles.iter().any(|c| c.name == b.partner) {
                return Err(Error::invalid(format!(
                    "bubbles.{}: no partner family '{}'",
                    b.name, b.partner
                )));
            }
        }
        let representations = raw
            .representations
            .into_iter()
            .map(|r| RepresentationDecl {
                name: r.name,
                kind: r.kind,
                roles: r.roles,
                target: r.target,
                images: r.images,
                even_only: r.even_only,
                certificate: r.certificate.into(),
                even_certificate: r.even_certificate.map(Certificate::from),
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
                let mut derived = BTreeMap::new();
                for (g, cases) in l.derived {
                    let Some(gen_decl) = generators.iter().find(|x| x.name == g) else {
                        return Err(Error::invalid(format!("lean: no generator '{g}'")));
                    };
                    let vars = &gen_decl.vars;
                    let mut out = Vec::new();
                    for c in cases {
                        if !c.when.is_empty() {
                            crate::expr::parse_cond(&c.when, vars)?;
                        }
                        if !c.sign.is_empty() {
                            crate::expr::parse_int_expr(&c.sign, vars)?;
                        }
                        let mut layers = Vec::new();
                        for l in c.layers {
                            if !(3..=4).contains(&l.len()) {
                                return Err(Error::invalid(format!(
                                    "lean: a layer of {g} needs left letters, shape, right \
                                     letters and optionally a repeat count"
                                )));
                            }
                            for part in &l {
                                for e in braced(part) {
                                    if !vars.contains(&e) {
                                        crate::expr::parse_int_expr(&e, vars)?;
                                    }
                                }
                            }
                            if let Some(n) = l.get(3) {
                                crate::expr::parse_int_expr(n, vars)?;
                            }
                            layers.push([
                                l[0].clone(),
                                l[1].clone(),
                                l[2].clone(),
                                l.get(3).cloned().unwrap_or_default(),
                            ]);
                        }
                        out.push(LeanDerived {
                            when: c.when,
                            sign: c.sign,
                            negate: c.negate,
                            layers,
                        });
                    }
                    derived.insert(g, out);
                }
                for (r, x) in &l.relations {
                    if !relations.iter().any(|y| &y.name == r) {
                        return Err(Error::invalid(format!("lean: no relation '{r}'")));
                    }
                    for l in [&x.layering, &x.pattern_layering] {
                        if !["canonical", "left-first", "right-first", "bubbles-first"]
                            .contains(&l.as_str())
                        {
                            return Err(Error::invalid(format!(
                                "lean: relation {r} has unknown layering '{l}'"
                            )));
                        }
                    }
                    if !["rw", "exact", "tactic"].contains(&x.proof.as_str()) {
                        return Err(Error::invalid(format!(
                            "lean: relation {r} has unknown proof '{}'",
                            x.proof
                        )));
                    }
                }
                Some(LeanDecl {
                    encoding: l.encoding,
                    module: l.module,
                    namespace: l.namespace,
                    open: l.open,
                    generators: l.generators,
                    derived,
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
                                    pattern_layering: r.pattern_layering,
                                    proof: r.proof,
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
            bubbles,
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
            if g.written != g.name {
                o.push_str(&format!("written = {}\n", q(&g.written)));
            }
        }
        for b in &self.bubbles {
            o.push_str(&format!("\n[bubbles.{}]\n", b.name));
            o.push_str(&format!("colours = {}\n", q(&b.var)));
            o.push_str(&format!("word = {}\n", q(&b.word)));
            o.push_str(&format!("index = {}\n", q(&b.index.print())));
            o.push_str(&format!("partner = {}\n", q(&b.partner)));
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
            if !r.ints.is_empty() {
                let is: Vec<String> = r
                    .ints
                    .iter()
                    .map(|(v, lo, hi)| {
                        format!("{v} = {}", q(&format!("{}..{}", lo.print(), hi.print())))
                    })
                    .collect();
                o.push_str(&format!("ints = {{ {} }}\n", is.join(", ")));
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
            if !roles.is_empty() {
                o.push_str(&format!("roles = {{ {} }}\n", roles.join(", ")));
            }
            if !r.target.is_empty() {
                o.push_str(&format!("target = {}\n", q(&r.target)));
            }
            if r.even_only {
                o.push_str("even_only = true\n");
            }
            if !r.images.is_empty() {
                let im: Vec<String> = r
                    .images
                    .iter()
                    .map(|(k, v)| format!("{k} = {}", q(v)))
                    .collect();
                o.push_str(&format!("images = {{ {} }}\n", im.join(", ")));
            }
            let certs = std::iter::once(("certificate", &r.certificate))
                .chain(r.even_certificate.iter().map(|c| ("even_certificate", c)));
            for (key, c) in certs {
                o.push_str(&format!("{key}.status = {}\n", q(&c.status)));
                if !c.reference.is_empty() {
                    o.push_str(&format!("{key}.reference = {}\n", q(&c.reference)));
                }
                if !c.hypotheses.is_empty() {
                    o.push_str(&format!("{key}.hypotheses = {}\n", q(&c.hypotheses)));
                }
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
            for (name, cases) in &l.derived {
                for c in cases {
                    o.push_str(&format!("\n[[lean.derived.{name}]]\n"));
                    if !c.when.is_empty() {
                        o.push_str(&format!("when = {}\n", q(&c.when)));
                    }
                    if !c.sign.is_empty() {
                        o.push_str(&format!("sign = {}\n", q(&c.sign)));
                    }
                    if c.negate {
                        o.push_str("negate = true\n");
                    }
                    let ls: Vec<String> = c
                        .layers
                        .iter()
                        .map(|l| {
                            let n = if l[3].is_empty() { 3 } else { 4 };
                            let parts: Vec<String> = l[..n].iter().map(|x| q(x)).collect();
                            format!("  [{}],\n", parts.join(", "))
                        })
                        .collect();
                    o.push_str(&format!("layers = [\n{}]\n", ls.concat()));
                }
            }
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
                if r.pattern_layering != "canonical" {
                    o.push_str(&format!("pattern_layering = {}\n", q(&r.pattern_layering)));
                }
                if r.proof != "rw" {
                    o.push_str(&format!("proof = {}\n", q(&r.proof)));
                }
            }
        }
        o
    }
}

/// The contents of each `{…}` in `s`, in order.
pub fn braced(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = s;
    while let Some(a) = rest.find('{') {
        let Some(b) = rest[a..].find('}') else { break };
        out.push(rest[a + 1..a + b].to_string());
        rest = &rest[a + b + 1..];
    }
    out
}
