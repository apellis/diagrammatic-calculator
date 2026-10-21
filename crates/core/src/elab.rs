//! Elaboration: turning a syntax tree into an explicit term in a context.
//!
//! Every subtree is elaborated *at a source object*: the result is the sum of words starting
//! there. A whole term written without an idempotent is the sum over the context's objects. An
//! idempotent `e(i)` at a source other than `i` is zero, which is how products of mismatched
//! pieces vanish.

use crate::context::Context;
use crate::error::{Error, Result};
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
}

impl<R: Ring> Default for Scope<'_, R> {
    fn default() -> Self {
        Scope {
            colour_vars: vec![],
            defs: None,
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
            if !ok {
                return Err(first_err.unwrap());
            }
            out.push(total);
        }
        Ok(out)
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
            Ast::Pow(b, _) => self.collect_objects(b, scope, out)?,
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
                    let t = self.elab_at(x, &part, scope)?;
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
        if let Some(g) = self.pres.generator(name) {
            let pos = match sub {
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
            let layer = self.match_layer(g, src, pos - 1).ok_or_else(|| {
                Error::invalid(format!(
                    "{name}_{pos} does not fit the object e({})",
                    self.fmt_object(src)
                ))
            })?;
            let w = Word {
                source: src.clone(),
                layers: vec![layer],
            };
            return Ok(self.monomial_term(&w, self.coeff.one()));
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
        if let Some(v) = self.coeff.var_named(&full) {
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

    /// The number of strands a tensor factor spans, when the text determines it.
    fn width(&self, ast: &Ast, scope: &Scope<R>) -> Result<Option<usize>> {
        Ok(match ast {
            Ast::Idem(r) | Ast::Id(r) => Some(self.read_object(r, scope)?.len()),
            Ast::Name {
                name,
                sub,
                args: None,
            } => match self.pres.generator(name) {
                Some(g) => {
                    let pos = sub
                        .as_ref()
                        .and_then(|s| s.parse::<usize>().ok())
                        .unwrap_or(1);
                    Some(pos + self.source_len(g).max(1) - 1)
                }
                None => match scope.defs.and_then(|d| d.get(name)) {
                    Some(t) => t.terms.first().map(|(w, _)| w.source.len()),
                    None => None,
                },
            },
            Ast::Pow(b, _) => self.width(b, scope)?,
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
