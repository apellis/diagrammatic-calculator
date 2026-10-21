//! A context: a presentation instantiated at a Cartan datum, a weight, a ground ring and
//! parameter values. Typing, canonical layering, composition and printing of terms happen here.

use crate::cartan::Cartan;
use crate::error::{Error, Result};
use crate::expr::ColourEnv;
use crate::poly::{Monomial, Poly, PolyRing};
use crate::pres::{Colour, ColourSpec, ObjectSpec, PatColour, Presentation};
use crate::ring::Ring;
use crate::term::{Layer, Letter, Object, Term, Word};
use crate::text::{self, Ast};
use std::collections::BTreeMap;

/// The choices that instantiate a presentation, as written on a workspace's `context` line:
/// `context klr type=A2 nu=2i+j ring=QQ`.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ContextSpec {
    pub presentation: String,
    /// A Cartan type such as `A2`, `B2`, `G2`.
    pub cartan_type: Option<String>,
    /// Colour names, in the order of the simple roots.
    pub colour_names: Option<Vec<String>>,
    /// Colours declared odd.
    pub odd: Vec<String>,
    /// The weight `ν`: `2i+j`, or a number of strands for one-colour presentations.
    pub weight: Option<String>,
    /// `ZZ`, `QQ`, `GF(p)` or `ZZ[I]`.
    pub ring: String,
    /// Scalar names available in terms: parameters such as `t_ij` and unknowns such as `c1`.
    pub scalars: Vec<String>,
    /// Parameter values, such as `Q_ij = "u^2 + v"`.
    pub params: BTreeMap<String, String>,
}

fn split_words(line: &str) -> Result<Vec<String>> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for c in line.chars() {
        match c {
            '"' => quoted = !quoted,
            c if c.is_whitespace() && !quoted => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if quoted {
        return Err(Error::syntax("unclosed quote in context", line.len()));
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    Ok(out)
}

impl ContextSpec {
    /// Reads `klr type=A2 nu=2i+j ring=QQ` (the part after the word `context`).
    pub fn parse(s: &str) -> Result<ContextSpec> {
        let words = split_words(s)?;
        let mut spec = ContextSpec {
            ring: "QQ".into(),
            ..Default::default()
        };
        let mut it = words.into_iter();
        spec.presentation = it
            .next()
            .ok_or_else(|| Error::invalid("context needs a presentation"))?;
        let list = |v: &str| {
            v.split(',')
                .filter(|x| !x.is_empty())
                .map(String::from)
                .collect()
        };
        for w in it {
            let (k, v) = w.split_once('=').ok_or_else(|| {
                Error::invalid(format!("context entry '{w}' should be key=value"))
            })?;
            match k {
                "type" => spec.cartan_type = Some(v.into()),
                "colours" | "colors" => spec.colour_names = Some(list(v)),
                "odd" => spec.odd = list(v),
                "nu" | "n" => spec.weight = Some(v.into()),
                "ring" => spec.ring = v.into(),
                "scalars" => spec.scalars = list(v),
                k if k.contains('_') => {
                    spec.params.insert(k.into(), v.into());
                }
                _ => return Err(Error::invalid(format!("unknown context key '{k}'"))),
            }
        }
        Ok(spec)
    }

    /// Canonical text form; `parse` of it gives the same spec.
    pub fn print(&self) -> String {
        let mut s = self.presentation.clone();
        let q = |v: &str| {
            if v.contains(char::is_whitespace) {
                format!("\"{v}\"")
            } else {
                v.to_string()
            }
        };
        if let Some(t) = &self.cartan_type {
            s.push_str(&format!(" type={t}"));
        }
        if let Some(c) = &self.colour_names {
            s.push_str(&format!(" colours={}", c.join(",")));
        }
        if !self.odd.is_empty() {
            s.push_str(&format!(" odd={}", self.odd.join(",")));
        }
        if let Some(w) = &self.weight {
            s.push_str(&format!(" nu={w}"));
        }
        s.push_str(&format!(" ring={}", self.ring));
        if !self.scalars.is_empty() {
            s.push_str(&format!(" scalars={}", self.scalars.join(",")));
        }
        for (k, v) in &self.params {
            s.push_str(&format!(" {k}={}", q(v)));
        }
        s
    }
}

/// Values of one parameter family by ordered pair of colours.
pub type ParamTable<E> = BTreeMap<(u16, u16), Poly<E>>;

/// A presentation instantiated over a ground ring `R`.
#[derive(Clone, Debug)]
pub struct Context<R: Ring> {
    pub pres: Presentation,
    pub spec: ContextSpec,
    pub colours: Vec<Colour>,
    pub cartan: Option<Cartan>,
    /// Multiplicity of each colour in `ν`.
    pub weight: Option<Vec<u32>>,
    /// The objects a word without an idempotent ranges over.
    pub objects: Vec<Object>,
    pub base: R,
    /// Coefficients: polynomials in the declared scalars.
    pub coeff: PolyRing<R>,
    /// Parameter polynomials live in `params_ring`: variables `u, v, w`, then the scalars.
    pub params_ring: PolyRing<R>,
    /// Parameter values by name and ordered pair of colours.
    pub params: BTreeMap<String, ParamTable<R::Elem>>,
}

const DEFAULT_COLOUR_NAMES: [&str; 12] =
    ["i", "j", "k", "l", "m", "n", "o", "p", "r", "s", "t", "u"];
pub const PARAM_VARS: [&str; 3] = ["u", "v", "w"];

impl<R: Ring> Context<R> {
    pub fn new(pres: Presentation, spec: ContextSpec, base: R) -> Result<Self> {
        let cartan = match &spec.cartan_type {
            Some(t) => Some(Cartan::of_type(t)?),
            None => None,
        };
        let mut colours = match &pres.colours {
            ColourSpec::Fixed(cs) => cs.clone(),
            ColourSpec::Cartan => {
                let c = cartan.as_ref().ok_or_else(|| {
                    Error::invalid(format!(
                        "presentation {} needs a Cartan type (type=A2)",
                        pres.name
                    ))
                })?;
                let names: Vec<String> = match &spec.colour_names {
                    Some(n) => n.clone(),
                    None => DEFAULT_COLOUR_NAMES
                        .iter()
                        .take(c.rank())
                        .map(|s| s.to_string())
                        .collect(),
                };
                if names.len() != c.rank() {
                    return Err(Error::invalid(format!(
                        "type {} has rank {} but {} colour names were given",
                        c.name,
                        c.rank(),
                        names.len()
                    )));
                }
                names
                    .into_iter()
                    .map(|name| Colour {
                        name,
                        odd: false,
                        oriented: false,
                    })
                    .collect()
            }
        };
        for o in &spec.odd {
            let c = colours
                .iter_mut()
                .find(|c| &c.name == o)
                .ok_or_else(|| Error::invalid(format!("odd colour '{o}' is not a colour")))?;
            c.odd = true;
        }
        for (k, c) in colours.iter().enumerate() {
            if colours[..k].iter().any(|d| d.name == c.name) {
                return Err(Error::invalid(format!("colour '{}' named twice", c.name)));
            }
            if c.name == "e" || c.name == "I" {
                return Err(Error::invalid("colours may not be named 'e' or 'I'"));
            }
        }
        let coeff = PolyRing::new(
            base.clone(),
            &spec.scalars.iter().map(String::as_str).collect::<Vec<_>>(),
        );
        let mut pnames: Vec<&str> = PARAM_VARS.to_vec();
        pnames.extend(spec.scalars.iter().map(String::as_str));
        let params_ring = PolyRing::new(base.clone(), &pnames);
        let mut ctx = Context {
            pres,
            spec,
            colours,
            cartan,
            weight: None,
            objects: vec![],
            base,
            coeff,
            params_ring,
            params: BTreeMap::new(),
        };
        if let Some(w) = ctx.spec.weight.clone() {
            let m = ctx.parse_weight(&w)?;
            if ctx.pres.objects == ObjectSpec::Weight {
                ctx.objects = sequences(&m);
            }
            ctx.weight = Some(m);
        }
        ctx.init_params()?;
        Ok(ctx)
    }

    /// Reads `2i+j` (or a bare number for a one-colour presentation).
    pub fn parse_weight(&self, w: &str) -> Result<Vec<u32>> {
        let mut m = vec![0u32; self.colours.len()];
        if let Ok(n) = w.trim().parse::<u32>() {
            if self.colours.len() != 1 {
                return Err(Error::invalid(
                    "a bare number of strands needs a one-colour presentation",
                ));
            }
            m[0] = n;
            return Ok(m);
        }
        for part in w.split('+') {
            let part = part.trim();
            let digits: String = part.chars().take_while(|c| c.is_ascii_digit()).collect();
            let name = &part[digits.len()..];
            let k: u32 = if digits.is_empty() {
                1
            } else {
                digits.parse().unwrap()
            };
            let c = self
                .colour_index(name)
                .ok_or_else(|| Error::invalid(format!("'{name}' in nu={w} is not a colour")))?;
            m[c] += k;
        }
        Ok(m)
    }

    pub fn colour_index(&self, name: &str) -> Option<usize> {
        self.colours.iter().position(|c| c.name == name)
    }

    fn init_params(&mut self) -> Result<()> {
        let n = self.colours.len();
        let pres_params = self.pres.parameters.clone();
        // Base parameters first, derived ones after.
        for p in pres_params.iter().filter(|p| p.derived.is_none()) {
            let mut table = BTreeMap::new();
            for a in 0..n {
                for b in 0..n {
                    if a == b {
                        continue;
                    }
                    let key = format!(
                        "{}_{}{}",
                        p.name, self.colours[a].name, self.colours[b].name
                    );
                    let rkey = format!(
                        "{}_{}{}",
                        p.name, self.colours[b].name, self.colours[a].name
                    );
                    let poly = if let Some(t) = self.spec.params.get(&key) {
                        self.parse_param_poly(t)?
                    } else if let Some(t) = self.spec.params.get(&rkey) {
                        // Q_ab(u, v) = Q_ba(v, u)
                        let q = self.parse_param_poly(t)?;
                        self.swap_uv(&q)
                    } else {
                        self.default_param(&p.default, a, b)?
                    };
                    table.insert((a as u16, b as u16), poly);
                }
            }
            self.params.insert(p.name.clone(), table);
        }
        for p in pres_params.iter().filter(|p| p.derived.is_some()) {
            let d = p.derived.as_ref().unwrap();
            let inner = d
                .strip_prefix("qbar(")
                .and_then(|s| s.strip_suffix(')'))
                .ok_or_else(|| Error::invalid(format!("unknown derived parameter '{d}'")))?;
            let base = self
                .params
                .get(inner)
                .ok_or_else(|| Error::invalid(format!("qbar of unknown parameter '{inner}'")))?
                .clone();
            let mut table = BTreeMap::new();
            for (k, q) in base {
                table.insert(k, self.qbar(&q)?);
            }
            self.params.insert(p.name.clone(), table);
        }
        Ok(())
    }

    fn parse_param_poly(&self, t: &str) -> Result<Poly<R::Elem>> {
        let ast = text::parse(t)?;
        poly_of_ast(&ast, &self.params_ring)
    }

    fn swap_uv(&self, q: &Poly<R::Elem>) -> Poly<R::Elem> {
        self.params_ring.swap_vars(q, 0, 1)
    }

    /// `Q̄(u, v, w) = (Q(u, v) − Q(w, v)) / (u − w)`.
    fn qbar(&self, q: &Poly<R::Elem>) -> Result<Poly<R::Elem>> {
        let r = &self.params_ring;
        let qw = r.swap_vars(q, 0, 2);
        let diff = r.sub(q, &qw);
        r.div_by_difference(&diff, 0, 2)
            .ok_or_else(|| Error::invalid("Q(u,v) - Q(w,v) is not divisible by u - w"))
    }

    fn default_param(&self, d: &str, a: usize, b: usize) -> Result<Poly<R::Elem>> {
        let r = &self.params_ring;
        match d {
            "klq2" => {
                let c = self
                    .cartan
                    .as_ref()
                    .ok_or_else(|| Error::invalid("the default Q needs a Cartan type"))?;
                if c.form(a, b) == 0 {
                    return Ok(r.one());
                }
                let dab = -c.a[a][b];
                let dba = -c.a[b][a];
                let nv = r.nvars();
                let u = r.monomial(Monomial::var(nv, 0, dab as i32), r.base.one());
                let v = r.monomial(Monomial::var(nv, 1, dba as i32), r.base.one());
                Ok(r.add(&u, &v))
            }
            "" => Err(Error::invalid(
                "a parameter without default needs a value in the context",
            )),
            t => self.parse_param_poly(t),
        }
    }

    // -----------------------------------------------------------------------------------------
    // Typing.

    pub fn source_len(&self, g: usize) -> usize {
        self.pres.generators[g].source.len()
    }
    pub fn target_len(&self, g: usize) -> usize {
        self.pres.generators[g].target.len()
    }

    /// The layer placing generator `g` at 0-based position `pos` on `obj`, if it fits.
    pub fn match_layer(&self, g: usize, obj: &[Letter], pos: usize) -> Option<Layer> {
        let g_decl = &self.pres.generators[g];
        if pos + g_decl.source.len() > obj.len() {
            return None;
        }
        let mut binding: Vec<Option<u16>> = vec![None; g_decl.vars.len()];
        for (k, p) in g_decl.source.iter().enumerate() {
            let l = obj[pos + k];
            if l.up != p.up {
                return None;
            }
            match &p.colour {
                PatColour::Var(v) => match binding[*v] {
                    Some(c) if c != l.colour => return None,
                    _ => binding[*v] = Some(l.colour),
                },
                PatColour::Fixed(name) => {
                    if self.colour_index(name) != Some(l.colour as usize) {
                        return None;
                    }
                }
            }
        }
        let binding: Option<Vec<u16>> = binding.into_iter().collect();
        Some(Layer {
            generator: g as u16,
            pos: pos as u16,
            binding: binding?,
        })
    }

    /// The object after applying a layer.
    pub fn apply(&self, layer: &Layer, obj: &[Letter]) -> Object {
        let g_decl = &self.pres.generators[layer.generator as usize];
        let p = layer.pos as usize;
        let mut out: Object = obj[..p].to_vec();
        for t in &g_decl.target {
            let colour = match &t.colour {
                PatColour::Var(v) => layer.binding[*v],
                PatColour::Fixed(name) => self.colour_index(name).expect("fixed colour") as u16,
            };
            out.push(Letter { colour, up: t.up });
        }
        out.extend_from_slice(&obj[p + g_decl.source.len()..]);
        out
    }

    pub fn target(&self, w: &Word) -> Object {
        let mut obj = w.source.clone();
        for l in &w.layers {
            obj = self.apply(l, &obj);
        }
        obj
    }

    pub fn layer_degree(&self, l: &Layer) -> i64 {
        let b: Vec<usize> = l.binding.iter().map(|&c| c as usize).collect();
        self.pres.generators[l.generator as usize]
            .degree
            .eval(&b, self)
            .unwrap_or(0)
    }

    pub fn layer_odd(&self, l: &Layer) -> bool {
        let b: Vec<usize> = l.binding.iter().map(|&c| c as usize).collect();
        self.pres.generators[l.generator as usize]
            .parity
            .eval(&b, self)
            .unwrap_or(0)
            .rem_euclid(2)
            == 1
    }

    pub fn word_degree(&self, w: &Word) -> i64 {
        w.layers.iter().map(|l| self.layer_degree(l)).sum()
    }

    pub fn word_odd(&self, w: &Word) -> bool {
        w.layers.iter().filter(|l| self.layer_odd(l)).count() % 2 == 1
    }

    // -----------------------------------------------------------------------------------------
    // Canonical layering.

    /// Heights in the canonical layering: one more than the highest layer whose outputs it uses.
    pub fn levels(&self, w: &Word) -> Vec<u32> {
        let mut floor: Vec<u32> = vec![0; w.source.len()];
        let mut out = Vec::with_capacity(w.layers.len());
        for l in &w.layers {
            let p = l.pos as usize;
            let s = self.source_len(l.generator as usize);
            let t = self.target_len(l.generator as usize);
            let below = if s > 0 {
                floor[p..p + s].iter().copied().max().unwrap_or(0)
            } else {
                // A generator with no inputs sits above its neighbours, conservatively.
                let left = if p > 0 { floor[p - 1] } else { 0 };
                let right = floor.get(p).copied().unwrap_or(0);
                left.max(right)
            };
            let lev = below + 1;
            out.push(lev);
            floor.splice(p..p + s, std::iter::repeat_n(lev, t));
        }
        out
    }

    /// Whether `upper` (directly above `lower`) is entirely left of it, entirely right, or neither.
    fn relative(&self, lower: &Layer, upper: &Layer) -> Option<bool> {
        let (pl, tl) = (
            lower.pos as usize,
            self.target_len(lower.generator as usize),
        );
        let (pu, su) = (
            upper.pos as usize,
            self.source_len(upper.generator as usize),
        );
        if su == 0 && tl == 0 && pu == pl {
            return None;
        }
        if pu + su <= pl {
            Some(true)
        } else if pu >= pl + tl {
            Some(false)
        } else {
            None
        }
    }

    /// Moves every generator as low as possible, same-height generators left to right. Returns
    /// the canonical word and whether the Koszul signs of the moves multiply to −1.
    pub fn canonical(&self, w: &Word) -> (Word, bool) {
        let levels = self.levels(w);
        let mut items: Vec<(Layer, u32)> = w.layers.iter().cloned().zip(levels).collect();
        let mut sign = false;
        loop {
            let mut swapped = false;
            for i in 0..items.len().saturating_sub(1) {
                let (lower, ll) = (&items[i].0, items[i].1);
                let (upper, lu) = (&items[i + 1].0, items[i + 1].1);
                let Some(upper_left) = self.relative(lower, upper) else {
                    continue;
                };
                if !(lu < ll || (lu == ll && upper_left)) {
                    continue;
                }
                let (gl, gu) = (lower.generator as usize, upper.generator as usize);
                let (sl, tl) = (self.source_len(gl) as i64, self.target_len(gl) as i64);
                let (su, tu) = (self.source_len(gu) as i64, self.target_len(gu) as i64);
                let (pl, pu) = (lower.pos as i64, upper.pos as i64);
                let (new_lower_pos, new_upper_pos) = if upper_left {
                    (pu, pl + tu - su)
                } else {
                    (pu - tl + sl, pl)
                };
                if self.layer_odd(lower) && self.layer_odd(upper) {
                    sign = !sign;
                }
                let mut nl = upper.clone();
                nl.pos = new_lower_pos as u16;
                let mut nu = lower.clone();
                nu.pos = new_upper_pos as u16;
                items[i] = (nl, lu);
                items[i + 1] = (nu, ll);
                swapped = true;
            }
            if !swapped {
                break;
            }
        }
        (
            Word {
                source: w.source.clone(),
                layers: items.into_iter().map(|x| x.0).collect(),
            },
            sign,
        )
    }

    // -----------------------------------------------------------------------------------------
    // Term arithmetic. Coefficients live in `self.coeff`.

    /// Sorts, combines and drops zeros. Words must already be canonical.
    pub fn normalize(&self, mut v: Vec<(Word, Poly<R::Elem>)>) -> Term<Poly<R::Elem>> {
        v.sort_by(|a, b| a.0.cmp(&b.0));
        let mut out: Vec<(Word, Poly<R::Elem>)> = Vec::with_capacity(v.len());
        for (w, c) in v {
            match out.last_mut() {
                Some((lw, lc)) if *lw == w => *lc = self.coeff.add(lc, &c),
                _ => out.push((w, c)),
            }
        }
        out.retain(|(_, c)| !self.coeff.is_zero(c));
        Term { terms: out }
    }

    /// `c · w`, with `w` brought to canonical layering.
    pub fn monomial_term(&self, w: &Word, c: Poly<R::Elem>) -> Term<Poly<R::Elem>> {
        let (cw, neg) = self.canonical(w);
        let c = if neg { self.coeff.neg(&c) } else { c };
        self.normalize(vec![(cw, c)])
    }

    pub fn identity(&self, obj: Object) -> Term<Poly<R::Elem>> {
        self.normalize(vec![(Word::identity(obj), self.coeff.one())])
    }

    pub fn add(&self, a: &Term<Poly<R::Elem>>, b: &Term<Poly<R::Elem>>) -> Term<Poly<R::Elem>> {
        let mut v = a.terms.clone();
        v.extend(b.terms.iter().cloned());
        self.normalize(v)
    }

    pub fn neg(&self, a: &Term<Poly<R::Elem>>) -> Term<Poly<R::Elem>> {
        Term {
            terms: a
                .terms
                .iter()
                .map(|(w, c)| (w.clone(), self.coeff.neg(c)))
                .collect(),
        }
    }

    pub fn sub(&self, a: &Term<Poly<R::Elem>>, b: &Term<Poly<R::Elem>>) -> Term<Poly<R::Elem>> {
        self.add(a, &self.neg(b))
    }

    pub fn scale(&self, a: &Term<Poly<R::Elem>>, c: &Poly<R::Elem>) -> Term<Poly<R::Elem>> {
        self.normalize(
            a.terms
                .iter()
                .map(|(w, x)| (w.clone(), self.coeff.mul(x, c)))
                .collect(),
        )
    }

    /// `a ∘ b`: `b` below, `a` above. Pairs whose objects do not match contribute zero.
    pub fn compose(&self, a: &Term<Poly<R::Elem>>, b: &Term<Poly<R::Elem>>) -> Term<Poly<R::Elem>> {
        let mut out = Vec::new();
        for (wb, cb) in &b.terms {
            let tb = self.target(wb);
            for (wa, ca) in &a.terms {
                if wa.source != tb {
                    continue;
                }
                let mut layers = wb.layers.clone();
                layers.extend(wa.layers.iter().cloned());
                let (w, neg) = self.canonical(&Word {
                    source: wb.source.clone(),
                    layers,
                });
                let mut c = self.coeff.mul(ca, cb);
                if neg {
                    c = self.coeff.neg(&c);
                }
                out.push((w, c));
            }
        }
        self.normalize(out)
    }

    /// `a ⊗ b = (a ⊗ 1)(1 ⊗ b)`: `b` at the bottom on the right, `a` above it on the left.
    pub fn tensor(&self, a: &Term<Poly<R::Elem>>, b: &Term<Poly<R::Elem>>) -> Term<Poly<R::Elem>> {
        let mut out = Vec::new();
        for (wa, ca) in &a.terms {
            for (wb, cb) in &b.terms {
                let shift = wa.source.len() as u16;
                let mut source = wa.source.clone();
                source.extend_from_slice(&wb.source);
                let mut layers: Vec<Layer> = wb
                    .layers
                    .iter()
                    .map(|l| Layer {
                        pos: l.pos + shift,
                        ..l.clone()
                    })
                    .collect();
                layers.extend(wa.layers.iter().cloned());
                let (w, neg) = self.canonical(&Word { source, layers });
                let mut c = self.coeff.mul(ca, cb);
                if neg {
                    c = self.coeff.neg(&c);
                }
                out.push((w, c));
            }
        }
        self.normalize(out)
    }

    // -----------------------------------------------------------------------------------------
    // Printing.

    /// Colours of an object as written inside `e(...)`: run together when every name is one
    /// character and every strand points up, otherwise separated by spaces.
    pub fn fmt_object(&self, obj: &[Letter]) -> String {
        let short = obj
            .iter()
            .all(|l| l.up && self.colours[l.colour as usize].name.chars().count() == 1);
        let parts: Vec<String> = obj
            .iter()
            .map(|l| {
                let n = &self.colours[l.colour as usize].name;
                if l.up { n.clone() } else { format!("{n}*") }
            })
            .collect();
        if short {
            parts.concat()
        } else {
            parts.join(" ")
        }
    }

    /// The idempotent of an object is left out when it is the only object of the context.
    fn omit_idempotent(&self, obj: &[Letter]) -> bool {
        self.objects.len() == 1 && self.objects[0] == obj
    }

    /// The layers in printed order, top first: levels from the top down, and within a level
    /// left to right (so the leftmost piece is written first, as if it sat highest). Positions
    /// are adjusted to that reading. The flag is the Koszul sign between this reading and the
    /// canonical word.
    pub fn print_order(&self, w: &Word) -> (Vec<Layer>, bool) {
        let levels = self.levels(w);
        let mut groups: Vec<Vec<Layer>> = Vec::new();
        let mut sign = false;
        let mut i = 0;
        while i < w.layers.len() {
            let mut j = i;
            let mut shift: i64 = 0;
            let mut group = Vec::new();
            let mut odd_before = 0usize;
            while j < w.layers.len() && levels[j] == levels[i] {
                let l = &w.layers[j];
                let g = l.generator as usize;
                group.push(Layer {
                    pos: (l.pos as i64 - shift) as u16,
                    ..l.clone()
                });
                shift += self.target_len(g) as i64 - self.source_len(g) as i64;
                if self.layer_odd(l) {
                    if odd_before % 2 == 1 {
                        sign = !sign;
                    }
                    odd_before += 1;
                }
                j += 1;
            }
            groups.push(group);
            i = j;
        }
        (groups.into_iter().rev().flatten().collect(), sign)
    }

    /// A word without its coefficient: layers from the top, then the idempotent. Printing a
    /// coefficient with it needs the sign from [`Context::print_order`]; `fmt_term` does that.
    pub fn fmt_word(&self, w: &Word) -> String {
        let (layers, _) = self.print_order(w);
        let mut parts: Vec<String> = Vec::new();
        let mut i = 0;
        while i < layers.len() {
            let l = &layers[i];
            let mut j = i + 1;
            while j < layers.len() && layers[j] == *l {
                j += 1;
            }
            let count = j - i;
            let g = &self.pres.generators[l.generator as usize];
            let s = format!("{}_{}", g.name, l.pos + 1);
            parts.push(if count > 1 { format!("{s}^{count}") } else { s });
            i = j;
        }
        if !self.omit_idempotent(&w.source) || parts.is_empty() {
            if self.omit_idempotent(&w.source) {
                parts.push("1".into());
            } else {
                parts.push(format!("e({})", self.fmt_object(&w.source)));
            }
        }
        parts.join(" ")
    }

    /// Canonical text of a term.
    pub fn fmt_term(&self, t: &Term<Poly<R::Elem>>) -> String {
        if t.terms.is_empty() {
            return "0".into();
        }
        let mut s = String::new();
        for (k, (w, c)) in t.terms.iter().enumerate() {
            let c = &if self.print_order(w).1 {
                self.coeff.neg(c)
            } else {
                c.clone()
            };
            let neg = self.coeff.is_negative(c);
            let abs = if neg { self.coeff.neg(c) } else { c.clone() };
            if k == 0 {
                if neg {
                    s.push('-');
                }
            } else {
                s.push_str(if neg { " - " } else { " + " });
            }
            let ws = self.fmt_word(w);
            if self.coeff.is_one(&abs) {
                s.push_str(&ws);
            } else {
                let cs = self.coeff.fmt_elem(&abs);
                let cs = if self.coeff.needs_parens(&abs) {
                    format!("({cs})")
                } else {
                    cs
                };
                if ws == "1" {
                    s.push_str(&cs);
                } else {
                    s.push_str(&format!("{cs} {ws}"));
                }
            }
        }
        s
    }
}

impl<R: Ring> ColourEnv for Context<R> {
    fn form(&self, a: usize, b: usize) -> Result<i64> {
        self.cartan
            .as_ref()
            .map(|c| c.form(a, b))
            .ok_or_else(|| Error::invalid("no Cartan datum"))
    }
    fn cartan_entry(&self, a: usize, b: usize) -> Result<i64> {
        self.cartan
            .as_ref()
            .map(|c| c.a[a][b])
            .ok_or_else(|| Error::invalid("no Cartan datum"))
    }
    fn symmetrizer(&self, a: usize) -> Result<i64> {
        self.cartan
            .as_ref()
            .map(|c| c.d[a])
            .ok_or_else(|| Error::invalid("no Cartan datum"))
    }
    fn parity(&self, a: usize) -> i64 {
        self.colours[a].odd as i64
    }
}

/// All distinct sequences with the given colour multiplicities, in lexicographic order.
pub fn sequences(m: &[u32]) -> Vec<Object> {
    fn go(m: &mut [u32], cur: &mut Object, out: &mut Vec<Object>) {
        if m.iter().all(|&k| k == 0) {
            out.push(cur.clone());
            return;
        }
        for c in 0..m.len() {
            if m[c] > 0 {
                m[c] -= 1;
                cur.push(Letter::up(c));
                go(m, cur, out);
                cur.pop();
                m[c] += 1;
            }
        }
    }
    let mut out = Vec::new();
    go(&mut m.to_vec(), &mut Vec::new(), &mut out);
    out
}

/// Evaluates a syntax tree as a polynomial in `ring`'s variables (`name` or `name_sub`).
pub fn poly_of_ast<R: Ring>(a: &Ast, ring: &PolyRing<R>) -> Result<Poly<R::Elem>> {
    Ok(match a {
        Ast::Int(n) => ring
            .rational(n, &num_bigint::BigInt::from(1))
            .ok_or_else(|| Error::invalid(format!("{n} is not in the ring")))?,
        Ast::Frac(n, d) => ring
            .rational(n, d)
            .ok_or_else(|| Error::invalid(format!("{n}/{d} is not in the ring")))?,
        Ast::Imag => ring
            .sqrt_minus_one()
            .ok_or_else(|| Error::invalid("I needs the ring ZZ[I]"))?,
        Ast::Name {
            name,
            sub,
            args: None,
        } => {
            let full = match sub {
                Some(s) => format!("{name}_{s}"),
                None => name.clone(),
            };
            ring.var_named(&full)
                .ok_or_else(|| Error::invalid(format!("unknown scalar '{full}'")))?
        }
        Ast::Pow(b, e) => {
            let x = poly_of_ast(b, ring)?;
            if *e >= 0 {
                ring.pow(&x, *e as u32)
            } else {
                let xi = ring
                    .inv(&x)
                    .ok_or_else(|| Error::invalid("negative power of a non-unit"))?;
                ring.pow(&xi, e.unsigned_abs() as u32)
            }
        }
        Ast::Comp(items) => {
            let mut acc = ring.one();
            for x in items {
                acc = ring.mul(&acc, &poly_of_ast(x, ring)?);
            }
            acc
        }
        Ast::Sum(items) => {
            let mut acc = ring.zero();
            for (neg, x) in items {
                let p = poly_of_ast(x, ring)?;
                acc = if *neg {
                    ring.sub(&acc, &p)
                } else {
                    ring.add(&acc, &p)
                };
            }
            acc
        }
        _ => {
            return Err(Error::invalid(format!(
                "'{}' is not a polynomial",
                text::print(a)
            )));
        }
    })
}
