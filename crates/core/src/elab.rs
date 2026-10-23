//! Elaboration: turning a syntax tree into an explicit term in a context.
//!
//! Every subtree is elaborated *at a source object*: the result is the sum of words starting
//! there. A whole term written without an idempotent is the sum over the context's objects. An
//! idempotent `e(i)` at a source other than `i` is zero, which is how products of mismatched
//! pieces vanish.

use crate::context::Context;
use crate::error::{Error, Result};
use crate::expr::ColourQuery;
use crate::poly::Poly;
use crate::ring::Ring;
use crate::term::{Letter, Object, Term, Word};
use crate::text::{self, Ast};
use std::collections::BTreeMap;

pub type CTerm<R> = Term<Poly<<R as Ring>::Elem>>;
type Summand<R> = (Word, Poly<<R as Ring>::Elem>);

/// Names in scope besides the presentation's: colour variables of a relation schema and
/// definitions of a workspace.
pub struct Scope<'a, R: Ring> {
    pub colour_vars: Vec<(String, u16)>,
    pub defs: Option<&'a BTreeMap<String, CTerm<R>>>,
    /// Integer variables: summation indices, and the integer variables of a relation.
    pub ints: Vec<(String, i64)>,
    /// In a 2-category, the weight of the region right of the part being elaborated, where it
    /// differs from the context's `μ` (inside a tensor factor).
    pub region: Option<Vec<i64>>,
}

impl<R: Ring> Default for Scope<'_, R> {
    fn default() -> Self {
        Scope {
            colour_vars: vec![],
            defs: None,
            ints: vec![],
            region: None,
        }
    }
}

impl<'a, R: Ring> Scope<'a, R> {
    fn with_int(&self, v: &str, n: i64) -> Scope<'a, R> {
        let mut ints = self.ints.clone();
        ints.push((v.to_string(), n));
        Scope {
            colour_vars: self.colour_vars.clone(),
            defs: self.defs,
            ints,
            region: self.region.clone(),
        }
    }
    fn with_region(&self, region: Option<Vec<i64>>) -> Scope<'a, R> {
        Scope {
            colour_vars: self.colour_vars.clone(),
            defs: self.defs,
            ints: self.ints.clone(),
            region,
        }
    }
}

impl<R: Ring> Context<R> {
    /// Parses and elaborates text.
    pub fn parse_term(&self, src: &str) -> Result<CTerm<R>> {
        let ast = text::parse(src)?;
        self.elaborate(&ast, &Scope::default())
    }

    /// Elaborates a syntax tree, summing over the possible source objects.
    pub fn elaborate(&self, ast: &Ast, scope: &Scope<R>) -> Result<CTerm<R>> {
        Ok(self
            .elaborate_together(std::slice::from_ref(ast), scope)?
            .pop()
            .unwrap())
    }

    /// Elaborates several trees over the same source objects: those of the context, or else every
    /// object any of the trees names. Used for the two sides of an equation, so that `0` on one
    /// side ranges over the objects of the other.
    pub fn elaborate_together(&self, asts: &[Ast], scope: &Scope<R>) -> Result<Vec<CTerm<R>>> {
        // An unknown name is an error even where the rest of the term vanishes, since errors at
        // single objects are otherwise forgiven (a generator may not fit every object).
        for ast in asts {
            self.check_names(ast, scope)?;
        }
        let mut candidates: Vec<Object> = self.objects.clone();
        if candidates.is_empty() {
            for ast in asts {
                self.collect_objects(ast, scope, &mut candidates)?;
            }
            candidates.sort();
            candidates.dedup();
        }
        if candidates.is_empty() {
            return Err(Error::invalid(
                "cannot tell which object this term starts at: add an idempotent e(...) or set nu",
            ));
        }
        let mut out = Vec::with_capacity(asts.len());
        for ast in asts {
            let mut total = Term::zero();
            let mut first_err = None;
            let mut ok = false;
            for obj in &candidates {
                match self.elab_at(ast, obj, scope) {
                    Ok(t) => {
                        ok = true;
                        total = self.add(&total, &t);
                    }
                    Err(e) => {
                        first_err.get_or_insert(e);
                    }
                }
            }
            // forgiven only where something else is left: a term that comes out zero while a
            // generator fit nowhere it was asked for (x_7 e(iij)) is reported, not silently 0
            if !ok || (total.terms.is_empty() && first_err.is_some()) {
                return Err(first_err.unwrap());
            }
            out.push(total);
        }
        Ok(out)
    }

    fn check_names(&self, ast: &Ast, scope: &Scope<R>) -> Result<()> {
        match ast {
            Ast::Name {
                name,
                sub,
                args: None,
            } => {
                let full = match sub {
                    Some(s) => format!("{name}_{s}"),
                    None => name.clone(),
                };
                let known = !self.pres.generators_written(name).is_empty()
                    || self.pres.bubble(name).is_some()
                    || (sub.is_none() && scope.defs.is_some_and(|d| d.contains_key(name)))
                    || self.coeff.named(&full).is_some();
                if !known {
                    return Err(Error::invalid(format!("unknown name '{full}'")));
                }
            }
            Ast::Name {
                args: Some(args), ..
            } => {
                for a in args {
                    self.check_names(a, scope)?;
                }
            }
            Ast::Comp(xs) | Ast::Tensor(xs) => {
                for x in xs {
                    self.check_names(x, scope)?;
                }
            }
            Ast::Sum(xs) => {
                for (_, x) in xs {
                    self.check_names(x, scope)?;
                }
            }
            Ast::Pow(b, _) | Ast::PowExpr(b, _) => self.check_names(b, scope)?,
            Ast::Range { body, .. } => self.check_names(body, scope)?,
            _ => {}
        }
        Ok(())
    }

    fn collect_objects(&self, ast: &Ast, scope: &Scope<R>, out: &mut Vec<Object>) -> Result<()> {
        match ast {
            Ast::Idem(r) | Ast::Id(r) => out.push(self.read_object(r, scope)?),
            Ast::Comp(xs) | Ast::Tensor(xs) => {
                for x in xs {
                    self.collect_objects(x, scope, out)?;
                }
            }
            Ast::Sum(xs) => {
                for (_, x) in xs {
                    self.collect_objects(x, scope, out)?;
                }
            }
            Ast::Pow(b, _) | Ast::PowExpr(b, _) => self.collect_objects(b, scope, out)?,
            Ast::Range { body, .. } => self.collect_objects(body, scope, out)?,
            Ast::Name {
                name, args: None, ..
            } => {
                if let Some(t) = scope.defs.and_then(|d| d.get(name)) {
                    out.extend(t.terms.iter().map(|(w, _)| w.source.clone()));
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Reads colours as written in `e(...)` or `1_...`, resolving colour variables first.
    pub fn read_object(&self, raw: &str, scope: &Scope<R>) -> Result<Object> {
        let lookup = |name: &str| -> Option<u16> {
            scope
                .colour_vars
                .iter()
                .find(|(v, _)| v == name)
                .map(|(_, c)| *c)
                .or_else(|| self.colour_index(name).map(|c| c as u16))
        };
        let mut out = Vec::new();
        for tok in raw
            .split(|c: char| c == ',' || c.is_whitespace())
            .filter(|t| !t.is_empty())
        {
            let mut rest = tok;
            while !rest.is_empty() {
                // longest known name at the front
                let mut best: Option<(usize, u16)> = None;
                let names = scope
                    .colour_vars
                    .iter()
                    .map(|(v, _)| v.as_str())
                    .chain(self.colours.iter().map(|c| c.name.as_str()));
                for n in names {
                    if rest.starts_with(n) && best.is_none_or(|(l, _)| n.len() > l) {
                        best = Some((n.len(), lookup(n).unwrap()));
                    }
                }
                let (len, colour) = best.ok_or_else(|| {
                    Error::invalid(format!("'{rest}' in '{raw}' is not a colour"))
                })?;
                rest = &rest[len..];
                let up = if let Some(r) = rest.strip_prefix('*') {
                    rest = r;
                    false
                } else {
                    true
                };
                out.push(Letter { colour, up });
            }
        }
        Ok(out)
    }

    fn scalar_term(&self, c: Poly<R::Elem>, src: &Object) -> CTerm<R> {
        self.normalize(vec![(Word::identity(src.clone()), c)])
    }

    /// Elaborates `ast` as a sum of words starting at `src`.
    pub fn elab_at(&self, ast: &Ast, src: &Object, scope: &Scope<R>) -> Result<CTerm<R>> {
        match ast {
            Ast::Int(_) | Ast::Frac(_, _) | Ast::Imag => {
                let c = crate::context::poly_of_ast(ast, &self.coeff)?;
                Ok(self.scalar_term(c, src))
            }
            Ast::Idem(r) | Ast::Id(r) => {
                let obj = self.read_object(r, scope)?;
                Ok(if &obj == src {
                    self.identity(obj)
                } else {
                    Term::zero()
                })
            }
            Ast::Name { name, sub, args } => {
                self.elab_name(name, sub.as_deref(), args.as_deref(), src, scope)
            }
            Ast::Pow(b, e) => {
                if *e < 0 {
                    let c = crate::context::poly_of_ast(ast, &self.coeff)
                        .map_err(|_| Error::invalid("only scalars take negative powers"))?;
                    return Ok(self.scalar_term(c, src));
                }
                let items: Vec<Ast> = std::iter::repeat_n((**b).clone(), *e as usize).collect();
                if items.is_empty() {
                    return Ok(self.identity(src.clone()));
                }
                self.compose_at(&items, src, scope)
            }
            Ast::PowExpr(b, e) => {
                let n = self.eval_int(e, scope)?;
                if n < 0 {
                    // a unit such as −1 in a sign (−1)^{p(a)(h(a)+1)}
                    let c = crate::context::poly_of_ast(&Ast::Pow(b.clone(), n as _), &self.coeff)
                        .map_err(|_| {
                            Error::invalid(format!(
                                "the exponent {} is {n} here: only scalars take negative powers",
                                e.print()
                            ))
                        })?;
                    return Ok(self.scalar_term(c, src));
                }
                let items: Vec<Ast> = std::iter::repeat_n((**b).clone(), n as usize).collect();
                if items.is_empty() {
                    return Ok(self.identity(src.clone()));
                }
                self.compose_at(&items, src, scope)
            }
            Ast::Range { var, lo, hi, body } => {
                let (lo, hi) = (self.eval_int(lo, scope)?, self.eval_int(hi, scope)?);
                let mut acc = Term::zero();
                for v in lo..=hi {
                    let t = self.elab_at(body, src, &scope.with_int(var, v))?;
                    acc = self.add(&acc, &t);
                }
                Ok(acc)
            }
            Ast::Comp(items) => self.compose_at(items, src, scope),
            Ast::Tensor(items) => {
                let mut widths = Vec::new();
                for x in items {
                    widths.push(self.width(x, scope)?);
                }
                let known: usize = widths.iter().flatten().sum();
                let unknown = widths.iter().filter(|w| w.is_none()).count();
                if unknown > 1 || (unknown == 0 && known != src.len()) || known > src.len() {
                    if unknown == 0 {
                        return Ok(Term::zero());
                    }
                    return Err(Error::invalid(
                        "cannot tell how many strands each tensor factor has: add idempotents",
                    ));
                }
                let widths: Vec<usize> = widths
                    .iter()
                    .map(|w| w.unwrap_or(src.len() - known))
                    .collect();
                let mut start = 0;
                let mut acc: Option<CTerm<R>> = None;
                for (x, w) in items.iter().zip(widths) {
                    let part: Object = src[start..start + w].to_vec();
                    start += w;
                    // the region right of this factor: shifted by the strands of the later ones
                    let right = self
                        .local_region(scope)
                        .map(|r| self.shift_region(&r, &src[start..]));
                    let t = self.elab_at(x, &part, &scope.with_region(right))?;
                    acc = Some(match acc {
                        None => t,
                        Some(a) => self.tensor(&a, &t),
                    });
                }
                Ok(acc.unwrap())
            }
            Ast::Sum(items) => {
                let mut acc = Term::zero();
                for (neg, x) in items {
                    let t = self.elab_at(x, src, scope)?;
                    acc = if *neg {
                        self.sub(&acc, &t)
                    } else {
                        self.add(&acc, &t)
                    };
                }
                Ok(acc)
            }
        }
    }

    /// Composition, top factor first in `items`.
    fn compose_at(&self, items: &[Ast], src: &Object, scope: &Scope<R>) -> Result<CTerm<R>> {
        let (bottom, rest) = items.split_last().unwrap();
        let mut cur = self.elab_at(bottom, src, scope)?;
        for item in rest.iter().rev() {
            let mut by_target: BTreeMap<Object, Vec<Summand<R>>> = BTreeMap::new();
            for (w, c) in &cur.terms {
                by_target
                    .entry(self.target(w))
                    .or_default()
                    .push((w.clone(), c.clone()));
            }
            let mut next = Term::zero();
            for (t, part) in by_target {
                let upper = self.elab_at(item, &t, scope)?;
                let lower = Term { terms: part };
                next = self.add(&next, &self.compose(&upper, &lower));
            }
            cur = next;
        }
        Ok(cur)
    }

    fn elab_name(
        &self,
        name: &str,
        sub: Option<&str>,
        args: Option<&[Ast]>,
        src: &Object,
        scope: &Scope<R>,
    ) -> Result<CTerm<R>> {
        if let Some(args) = args {
            return self.elab_param(name, sub.unwrap_or(""), args, src, scope);
        }
        let cands = self.pres.generators_written(name);
        if !cands.is_empty() {
            let (pos_text, colours) = match sub {
                Some(s) => match s.split_once(',') {
                    Some((p, c)) => (Some(p.trim()), Some(c.trim())),
                    None => (Some(s), None),
                },
                None => (None, None),
            };
            let pos = match pos_text {
                Some(s) if s.chars().all(|c| c.is_ascii_digit()) => s
                    .parse::<usize>()
                    .map_err(|_| Error::invalid(format!("bad position '{s}'")))?,
                Some(s) => {
                    return Err(Error::invalid(format!(
                        "{name}_{s}: the subscript of a generator is a position"
                    )));
                }
                None => 1,
            };
            if pos == 0 {
                return Err(Error::invalid(format!("{name}_0: positions start at 1")));
            }
            let explicit = match colours {
                Some(c) => self.read_object(c, scope)?,
                None => vec![],
            };
            let layer = cands
                .iter()
                .find_map(|&g| self.match_layer_explicit(g, src, pos - 1, &explicit))
                .ok_or_else(|| {
                    let unbound = cands.iter().all(|&g| self.needs_colours(g));
                    if unbound && explicit.is_empty() {
                        Error::invalid(format!(
                            "{name}_{pos}: say which colour, as in {name}_{{{pos},i}}"
                        ))
                    } else {
                        Error::invalid(format!(
                            "{name}_{} does not fit the object e({})",
                            sub.unwrap_or("1"),
                            self.fmt_object(src)
                        ))
                    }
                })?;
            let w = Word {
                source: src.clone(),
                layers: vec![layer],
            };
            return Ok(self.monomial_term(&w, self.coeff.one()));
        }
        if self.pres.bubble(name).is_some() {
            let s = sub.ok_or_else(|| {
                Error::invalid(format!(
                    "a bubble is written {name}_{{i,m}}: a colour and a label"
                ))
            })?;
            let (c, m) = s.split_once(',').ok_or_else(|| {
                Error::invalid(format!(
                    "a bubble is written {name}_{{i,m}}: a colour and a label"
                ))
            })?;
            let colour = self.read_object(c.trim(), scope)?;
            if colour.len() != 1 || !colour[0].up {
                return Err(Error::invalid(format!(
                    "{name}_{{{s}}}: the first entry is one colour"
                )));
            }
            let label = crate::expr::parse_term_int(m.trim())?;
            let label = self.eval_int(&label, scope)?;
            // a bubble beside strands sits in the rightmost region
            let region = self.local_region(scope);
            let b = self.bubble_term(name, colour[0].colour, label, region.as_deref())?;
            return Ok(self.tensor(&self.identity(src.clone()), &b));
        }
        if sub.is_none()
            && let Some(t) = scope.defs.and_then(|d| d.get(name))
        {
            let part: Vec<_> = t
                .terms
                .iter()
                .filter(|(w, _)| &w.source == src)
                .cloned()
                .collect();
            return Ok(Term { terms: part });
        }
        let full = match sub {
            Some(s) => format!("{name}_{s}"),
            None => name.to_string(),
        };
        if let Some(v) = self.coeff.named(&full) {
            return Ok(self.scalar_term(v, src));
        }
        Err(Error::invalid(format!("unknown name '{full}'")))
    }

    /// A parameter polynomial applied to morphisms: `Q_ab(x_1, x_2)`.
    fn elab_param(
        &self,
        name: &str,
        sub: &str,
        args: &[Ast],
        src: &Object,
        scope: &Scope<R>,
    ) -> Result<CTerm<R>> {
        let decl = self
            .pres
            .parameter(name)
            .ok_or_else(|| Error::invalid(format!("unknown parameter '{name}'")))?;
        if args.len() != decl.vars.len() {
            return Err(Error::invalid(format!(
                "{name} takes {} arguments",
                decl.vars.len()
            )));
        }
        let cols = self.read_object(sub, scope)?;
        if cols.len() != 2 || cols[0].colour == cols[1].colour {
            return Err(Error::invalid(format!(
                "{name}_{sub} needs two distinct colours"
            )));
        }
        let poly = self.params[name]
            .get(&(cols[0].colour, cols[1].colour))
            .ok_or_else(|| Error::invalid(format!("no value for {name}_{sub}")))?
            .clone();
        let argt: Vec<CTerm<R>> = args
            .iter()
            .map(|a| self.elab_at(a, src, scope))
            .collect::<Result<_>>()?;
        let nparam = crate::context::PARAM_VARS.len();
        let mut acc = Term::zero();
        for (m, c) in &poly.terms {
            // coefficient: the scalar part of the monomial
            let scal = crate::poly::Monomial::from(&m.0[nparam..]);
            let coef = self.coeff.monomial(scal, c.clone());
            let mut t = self.scalar_term(coef, src);
            for (k, a) in argt.iter().enumerate() {
                for _ in 0..m.0[k] {
                    t = self.compose(a, &t);
                }
            }
            if m.0[argt.len()..nparam].iter().any(|&e| e != 0) {
                return Err(Error::invalid(format!(
                    "{name} uses more variables than it has arguments"
                )));
            }
            acc = self.add(&acc, &t);
        }
        Ok(acc)
    }

    /// Evaluates an integer expression of a term: indices from the scope, `h(c)` at the
    /// context's `μ` (for a relation instance, the region right of the relation).
    pub fn eval_int(&self, e: &crate::expr::TermInt, scope: &Scope<R>) -> Result<i64> {
        let var = |v: &str| {
            scope
                .ints
                .iter()
                .rev()
                .find(|(n, _)| n == v)
                .map(|(_, x)| *x)
        };
        let index = |c: &str| -> Result<usize> {
            scope
                .colour_vars
                .iter()
                .find(|(v, _)| v == c)
                .map(|(_, x)| *x as usize)
                .or_else(|| self.colour_index(c))
                .ok_or_else(|| Error::invalid(format!("'{c}' is not a colour")))
        };
        let colour = |q: ColourQuery| -> Result<i64> {
            match q {
                ColourQuery::Pair(c) => {
                    let idx = index(c)?;
                    let mu = self.mu.as_ref().ok_or_else(|| {
                        Error::invalid("h(...) needs region weights: give the context mu=...")
                    })?;
                    Ok(mu[idx])
                }
                ColourQuery::Par(c) => Ok(self.colours[index(c)?].odd as i64),
                ColourQuery::Entry(c, d) => {
                    let (a, b) = (index(c)?, index(d)?);
                    let cartan = self
                        .cartan
                        .as_ref()
                        .ok_or_else(|| Error::invalid("a Cartan entry a_cd needs a Cartan type"))?;
                    Ok(cartan.a[a][b])
                }
            }
        };
        e.eval(&var, &colour)
    }

    /// The region right of the part being elaborated.
    fn local_region(&self, scope: &Scope<R>) -> Option<Vec<i64>> {
        scope.region.clone().or_else(|| self.mu.clone())
    }

    /// The bubble `name_{colour,label}` in the region `region`: the family's word for a label
    /// `≥ 0`, else the fake bubble of the infinite Grassmannian relation, or zero.
    pub fn bubble_term(
        &self,
        name: &str,
        colour: u16,
        label: i64,
        region: Option<&[i64]>,
    ) -> Result<CTerm<R>> {
        let fam = self
            .pres
            .bubble(name)
            .ok_or_else(|| Error::invalid(format!("no bubble family '{name}'")))?
            .clone();
        let region = region.ok_or_else(|| {
            Error::invalid("bubbles need region weights: give the context mu=...")
        })?;
        let at = |m: i64| -> Scope<R> {
            Scope {
                colour_vars: vec![(fam.var.clone(), colour)],
                defs: None,
                ints: vec![("m".to_string(), m)],
                region: Some(region.to_vec()),
            }
        };
        let mut local = self.clone();
        local.mu = Some(region.to_vec());
        let index = |f: &crate::pres::BubbleFamily, m: i64| -> Result<i64> {
            local.eval_int(
                &f.index,
                &Scope {
                    colour_vars: vec![(f.var.clone(), colour)],
                    defs: None,
                    ints: vec![("m".to_string(), m)],
                    region: None,
                },
            )
        };
        let empty: Object = vec![];
        if label >= 0 {
            let ast = text::parse(&fam.word)?;
            return local.elab_at(&ast, &empty, &at(label));
        }
        let k = index(&fam, label)?;
        if k < 0 {
            return Ok(Term::zero());
        }
        // f_0 = 1, f_k = -Σ_{j=1}^k p_j f_{k-j}, p_j the partner's real bubble of index j
        let partner = self
            .pres
            .bubble(&fam.partner)
            .ok_or_else(|| Error::invalid(format!("no bubble family '{}'", fam.partner)))?
            .clone();
        let base = index(&partner, 0)?;
        let past = text::parse(&partner.word)?;
        let mut real = Vec::new();
        for j in 1..=k {
            let m = j - base;
            real.push(if m < 0 {
                Term::zero()
            } else {
                let sc = Scope {
                    colour_vars: vec![(partner.var.clone(), colour)],
                    defs: None,
                    ints: vec![("m".to_string(), m)],
                    region: Some(region.to_vec()),
                };
                local.elab_at(&past, &empty, &sc)?
            });
        }
        let mut f: Vec<CTerm<R>> = vec![local.identity(empty.clone())];
        for kk in 1..=k as usize {
            let mut acc = Term::zero();
            for j in 1..=kk {
                acc = local.add(&acc, &local.compose(&real[j - 1], &f[kk - j]));
            }
            f.push(local.neg(&acc));
        }
        Ok(f.pop().unwrap())
    }

    /// The number of strands a tensor factor spans, when the text determines it.
    fn width(&self, ast: &Ast, scope: &Scope<R>) -> Result<Option<usize>> {
        Ok(match ast {
            Ast::Idem(r) | Ast::Id(r) => Some(self.read_object(r, scope)?.len()),
            Ast::Name {
                name,
                sub,
                args: None,
            } => match self.pres.generators_written(name).first() {
                Some(&g) => {
                    let pos = sub
                        .as_ref()
                        .and_then(|s| s.split(',').next())
                        .and_then(|s| s.trim().parse::<usize>().ok())
                        .unwrap_or(1);
                    if self.source_len(g) == 0 {
                        Some(pos - 1)
                    } else {
                        Some(pos + self.source_len(g) - 1)
                    }
                }
                None => match scope.defs.and_then(|d| d.get(name)) {
                    Some(t) => t.terms.first().map(|(w, _)| w.source.len()),
                    None => None,
                },
            },
            Ast::Pow(b, _) | Ast::PowExpr(b, _) => self.width(b, scope)?,
            Ast::Range { body, .. } => self.width(body, scope)?,
            Ast::Comp(xs) => {
                let mut best = None;
                for x in xs.iter().rev() {
                    if let Some(w) = self.width(x, scope)? {
                        best = Some(best.map_or(w, |b: usize| b.max(w)));
                        if matches!(x, Ast::Idem(_) | Ast::Id(_)) {
                            return Ok(Some(w));
                        }
                    }
                }
                best
            }
            Ast::Sum(xs) => {
                let mut r = None;
                for (_, x) in xs {
                    if let Some(w) = self.width(x, scope)? {
                        r = Some(w);
                        break;
                    }
                }
                r
            }
            Ast::Tensor(xs) => {
                let mut total = 0;
                for x in xs {
                    match self.width(x, scope)? {
                        Some(w) => total += w,
                        None => return Ok(None),
                    }
                }
                Some(total)
            }
            _ => None,
        })
    }
}
