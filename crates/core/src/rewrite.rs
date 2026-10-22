//! Rewriting with relations: rules from relation instances, matching up to interchange inside a
//! rectangle, application with provenance maps, rewrite chains, and straightening to a basis
//! normal form.
//!
//! A rule is one direction of one instance of a relation, `pattern = replacement`. A match of a
//! rule in a word is a set of layers that interchange moves bring together into a rectangle
//! (consecutive layers × consecutive strands) holding exactly the pattern. Applying it replaces
//! the rectangle by each word of the replacement and brings the results back to canonical
//! layering. Every step records the moves, the rectangle and, for each resulting word, where its
//! layers came from: enough to replay the step elsewhere (the Lean export does).

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::conformance::{instance_terms, instances};
use crate::context::Context;
use crate::elab::CTerm;
use crate::error::{Error, Result};
use crate::perm::Perm;
use crate::poly::Poly;
use crate::ring::Ring;
use crate::term::{Layer, Term, Word};

/// One direction of one instance of a relation: `pattern = replacement`, the replacement a term
/// of canonical words with the pattern's source.
#[derive(Clone, Debug)]
pub struct Rule<R: Ring> {
    pub relation: String,
    /// The colours bound to the relation's colour variables.
    pub binding: Vec<usize>,
    /// The values of the relation's integer variables.
    pub ints: Vec<i64>,
    /// In a 2-category, the weight of the region right of the pattern, for a relation whose
    /// instances depend on it; the rule matches only there.
    pub region: Option<Vec<i64>>,
    /// From the left-hand side to the right-hand side.
    pub forward: bool,
    pub pattern: Word,
    pub replacement: CTerm<R>,
}

/// All rules of the presentation's relations, both directions where a side has a word to match.
///
/// The pattern of a direction is the word of that side with the most layers whose coefficient
/// in `lhs − rhs` is a unit (ties: the first in word order); identity words are never patterns.
pub fn rules<R: Ring>(ctx: &Context<R>) -> Result<Vec<Rule<R>>> {
    let regions: Vec<Vec<i64>> = ctx.mu.iter().cloned().collect();
    rules_for_regions(ctx, &regions)
}

/// The rules of [`rules`], with those of weight-dependent relations instantiated at each of
/// `regions` (as the region right of the pattern).
pub fn rules_for_regions<R: Ring>(ctx: &Context<R>, regions: &[Vec<i64>]) -> Result<Vec<Rule<R>>> {
    let mut free = ctx.clone();
    free.objects.clear();
    free.weight = None;
    let mut out = Vec::new();
    for rel in &ctx.pres.relations {
        if ctx.pres.relation_uses_region(rel) {
            for r in regions {
                let mut local = free.clone();
                local.mu = Some(r.clone());
                relation_rules(&local, rel, Some(r.clone()), &mut out)?;
            }
        } else {
            relation_rules(&free, rel, None, &mut out)?;
        }
    }
    Ok(out)
}

/// The rules for every region that occurs in `t` (and the context's `μ`).
pub fn rules_for_term<R: Ring>(ctx: &Context<R>, t: &CTerm<R>) -> Result<Vec<Rule<R>>> {
    rules_for_regions(ctx, &regions_in(ctx, t))
}

/// The weights of all regions of the words of `t`, sorted.
pub fn regions_in<R: Ring>(ctx: &Context<R>, t: &CTerm<R>) -> Vec<Vec<i64>> {
    let mut out: BTreeSet<Vec<i64>> = ctx.mu.iter().cloned().collect();
    if ctx.mu.is_none() {
        return vec![];
    }
    for (w, _) in &t.terms {
        let mut obj = w.source.clone();
        let mut add = |obj: &[crate::term::Letter]| {
            for p in 0..=obj.len() {
                out.extend(ctx.region(obj, p));
            }
        };
        add(&obj);
        for l in &w.layers {
            obj = ctx.apply(l, &obj);
            add(&obj);
        }
    }
    out.into_iter().collect()
}

fn relation_rules<R: Ring>(
    ctx: &Context<R>,
    rel: &crate::pres::Relation,
    region: Option<Vec<i64>>,
    out: &mut Vec<Rule<R>>,
) -> Result<()> {
    for b in instances(ctx, rel)? {
        let (lhs, rhs) = instance_terms(ctx, rel, &b)?;
        let d = ctx.sub(&lhs, &rhs);
        let fwd = lead(ctx, &lhs, &d);
        let bwd = lead(ctx, &rhs, &d).filter(|p| Some(p) != fwd.as_ref());
        for (forward, p) in [(true, fwd), (false, bwd)] {
            let Some(p) = p else { continue };
            out.push(Rule {
                relation: rel.name.clone(),
                binding: b.colours.clone(),
                ints: b.ints.clone(),
                region: region.clone(),
                forward,
                replacement: solve_for(ctx, &d, &p),
                pattern: p,
            });
        }
    }
    Ok(())
}

fn coeff_of<E: Clone>(t: &Term<E>, w: &Word) -> Option<E> {
    t.terms.iter().find(|(v, _)| v == w).map(|(_, c)| c.clone())
}

fn lead<R: Ring>(ctx: &Context<R>, side: &CTerm<R>, d: &CTerm<R>) -> Option<Word> {
    side.terms
        .iter()
        .map(|(w, _)| w)
        .filter(|w| !w.layers.is_empty())
        .filter(|w| coeff_of(d, w).is_some_and(|c| ctx.coeff.inv(&c).is_some()))
        .max_by(|a, b| a.layers.len().cmp(&b.layers.len()).then(b.cmp(a)))
        .cloned()
}

/// `p = −(d − c p) / c` where `c` is the coefficient of `p` in `d = 0`.
fn solve_for<R: Ring>(ctx: &Context<R>, d: &CTerm<R>, p: &Word) -> CTerm<R> {
    let c = coeff_of(d, p).expect("pattern occurs");
    let minv = ctx.coeff.neg(&ctx.coeff.inv(&c).expect("unit"));
    ctx.normalize(
        d.terms
            .iter()
            .filter(|(w, _)| w != p)
            .map(|(w, x)| (w.clone(), ctx.coeff.mul(x, &minv)))
            .collect(),
    )
}

// ---------------------------------------------------------------------------------------------
// The dependency graph of a word.

/// For each layer, the layers whose outputs it uses (a generator without inputs depends on the
/// layers that produced its neighbouring strands).
fn parents<R: Ring>(ctx: &Context<R>, w: &Word) -> Vec<Vec<usize>> {
    let mut owner: Vec<Option<usize>> = vec![None; w.source.len()];
    let mut out = Vec::with_capacity(w.layers.len());
    for (i, l) in w.layers.iter().enumerate() {
        let g = l.generator as usize;
        let (p, s, t) = (l.pos as usize, ctx.source_len(g), ctx.target_len(g));
        let mut ps: Vec<usize> = if s > 0 {
            owner[p..p + s].iter().flatten().copied().collect()
        } else {
            [p.checked_sub(1), Some(p)]
                .into_iter()
                .flatten()
                .filter_map(|q| owner.get(q).copied().flatten())
                .collect()
        };
        ps.sort_unstable();
        ps.dedup();
        out.push(ps);
        owner.splice(p..p + s, std::iter::repeat_n(Some(i), t));
    }
    out
}

struct Graph {
    parents: Vec<Vec<usize>>,
    children: Vec<Vec<usize>>,
}

impl Graph {
    fn new<R: Ring>(ctx: &Context<R>, w: &Word) -> Self {
        let parents = parents(ctx, w);
        let mut children = vec![vec![]; parents.len()];
        for (i, ps) in parents.iter().enumerate() {
            for &p in ps {
                children[p].push(i);
            }
        }
        Graph { parents, children }
    }

    /// The layers above `set` (reachable from it), not in it; `None` if one of them is also
    /// below a layer of `set` (the set is not convex).
    fn above(&self, set: &[usize]) -> Option<Vec<bool>> {
        let n = self.children.len();
        let mut in_set = vec![false; n];
        for &i in set {
            in_set[i] = true;
        }
        let mut above = vec![false; n];
        let mut stack = Vec::new();
        for &i in set {
            for &c in &self.children[i] {
                if !in_set[c] && !above[c] {
                    above[c] = true;
                    stack.push(c);
                }
            }
        }
        while let Some(i) = stack.pop() {
            for &c in &self.children[i] {
                if in_set[c] {
                    return None;
                }
                if !above[c] {
                    above[c] = true;
                    stack.push(c);
                }
            }
        }
        Some(above)
    }
}

// ---------------------------------------------------------------------------------------------
// Matching.

/// A region of a word: layers `layers.0 .. layers.1` and strands `strands.0 .. strands.1`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub layers: (usize, usize),
    pub strands: (usize, usize),
}

/// A match of a rule in a word.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Match {
    /// Index of the rule in the list it was matched from.
    pub rule: usize,
    /// The matched layers of the word, in the order of the pattern's layers.
    pub occurrence: Vec<usize>,
    /// The interchange moves bringing the pattern together, each the index of the lower of the
    /// two exchanged layers.
    pub swaps: Vec<usize>,
    /// Whether the moves cost a Koszul sign: the word is `±moved`.
    pub sign: bool,
    /// The word after the moves; its layers `at .. at + k` are the pattern placed at strand
    /// `window`.
    pub moved: Word,
    /// For each layer of `moved`, its index in the word.
    pub order: Vec<usize>,
    pub at: usize,
    pub window: usize,
}

type Label = (u16, Vec<u16>);

fn label(l: &Layer) -> Label {
    (l.generator, l.binding.clone())
}

/// All matches of `rules` in the canonical word `w`, optionally inside a rectangle of `w`: the
/// matched layers among its layers and the pattern's strands among its strands.
pub fn find_matches<R: Ring>(
    ctx: &Context<R>,
    rules: &[Rule<R>],
    w: &Word,
    rect: Option<&Rect>,
) -> Vec<Match> {
    let graph = Graph::new(ctx, w);
    let n = w.layers.len();
    let (l0, l1) = rect.map_or((0, n), |r| (r.layers.0, r.layers.1.min(n)));
    let mut out = Vec::new();
    for (ri, rule) in rules.iter().enumerate() {
        let k = rule.pattern.layers.len();
        let mut need: BTreeMap<Label, usize> = BTreeMap::new();
        for l in &rule.pattern.layers {
            *need.entry(label(l)).or_default() += 1;
        }
        let cand: Vec<usize> = (l0..l1)
            .filter(|&i| need.contains_key(&label(&w.layers[i])))
            .collect();
        if cand.len() < k {
            continue;
        }
        let mut sets = Vec::new();
        choose(w, &cand, 0, &mut need, k, &mut vec![], &mut sets);
        for set in sets {
            for m in match_set(ctx, &graph, ri, rule, w, &set) {
                let ok = rect.is_none_or(|r| {
                    m.window >= r.strands.0 && m.window + rule.pattern.source.len() <= r.strands.1
                });
                if ok {
                    out.push(m);
                }
            }
        }
    }
    out
}

fn choose(
    w: &Word,
    cand: &[usize],
    from: usize,
    need: &mut BTreeMap<Label, usize>,
    k: usize,
    cur: &mut Vec<usize>,
    out: &mut Vec<Vec<usize>>,
) {
    if cur.len() == k {
        out.push(cur.clone());
        return;
    }
    for j in from..cand.len() {
        if cand.len() - j < k - cur.len() {
            break;
        }
        let lab = label(&w.layers[cand[j]]);
        let left = need.get_mut(&lab).expect("candidate label");
        if *left == 0 {
            continue;
        }
        *left -= 1;
        cur.push(cand[j]);
        choose(w, cand, j + 1, need, k, cur, out);
        cur.pop();
        *need.get_mut(&lab).unwrap() += 1;
    }
}

/// The matches of one rule on one set of layers (sorted indices).
fn match_set<R: Ring>(
    ctx: &Context<R>,
    graph: &Graph,
    ri: usize,
    rule: &Rule<R>,
    w: &Word,
    set: &[usize],
) -> Vec<Match> {
    let k = set.len();
    let Some(above) = graph.above(set) else {
        return vec![];
    };
    let mut key = vec![0u8; w.layers.len()];
    for (i, kk) in key.iter_mut().enumerate() {
        if above[i] {
            *kk = 2;
        }
    }
    for &i in set {
        key[i] = 1;
    }
    let Some(r) = ctx.reorder(w, &key) else {
        return vec![];
    };
    let b = key.iter().filter(|&&x| x == 0).count();
    let mut obj = w.source.clone();
    for l in &r.word.layers[..b] {
        obj = ctx.apply(l, &obj);
    }
    let src = &rule.pattern.source;
    let m = src.len();
    let mut out = Vec::new();
    if obj.len() < m {
        return out;
    }
    'window: for p in 0..=obj.len() - m {
        if obj[p..p + m] != src[..] {
            continue;
        }
        if let Some(r) = &rule.region
            && ctx.region(&obj, p + m).as_ref() != Some(r)
        {
            continue;
        }
        let mut width = m;
        let mut layers = Vec::with_capacity(k);
        for l in &r.word.layers[b..b + k] {
            let g = l.generator as usize;
            let (s, t) = (ctx.source_len(g), ctx.target_len(g));
            let pos = l.pos as usize;
            if pos < p || pos - p + s > width {
                continue 'window;
            }
            width = width + t - s;
            layers.push(Layer {
                pos: (pos - p) as u16,
                ..l.clone()
            });
        }
        let sub = ctx.canonical_tracked(&Word {
            source: src.clone(),
            layers,
        });
        if sub.word.layers != rule.pattern.layers {
            continue;
        }
        let mut moved = r.word.clone();
        let mut order = r.origin.clone();
        for j in 0..k {
            moved.layers[b + j] = Layer {
                pos: sub.word.layers[j].pos + p as u16,
                ..sub.word.layers[j].clone()
            };
            order[b + j] = r.origin[b + sub.origin[j]];
        }
        let mut swaps = r.swaps.clone();
        swaps.extend(sub.swaps.iter().map(|&s| s + b));
        out.push(Match {
            rule: ri,
            occurrence: order[b..b + k].to_vec(),
            swaps,
            sign: r.sign ^ sub.sign,
            moved,
            order,
            at: b,
            window: p,
        });
    }
    out
}

// ---------------------------------------------------------------------------------------------
// Application.

/// One word of the result of applying a rule.
#[derive(Clone, Debug)]
pub struct Summand<R: Ring> {
    /// The coefficient of this word in the rule's replacement.
    pub coeff: Poly<R::Elem>,
    /// Whether bringing the word to canonical layering cost a Koszul sign.
    pub sign: bool,
    pub word: Word,
    /// Provenance: for each layer of `word`, its index in the rewritten word, or `None` if the
    /// rule created it.
    pub origin: Vec<Option<usize>>,
}

/// The words the rectangle of a match turns into.
pub fn apply_match<R: Ring>(ctx: &Context<R>, rule: &Rule<R>, m: &Match) -> Vec<Summand<R>> {
    let k = rule.pattern.layers.len();
    let mv = &m.moved;
    let mut out = Vec::new();
    for (rw, rc) in &rule.replacement.terms {
        let mut layers = mv.layers[..m.at].to_vec();
        let mut origin: Vec<Option<usize>> = m.order[..m.at].iter().map(|&i| Some(i)).collect();
        for l in &rw.layers {
            layers.push(Layer {
                pos: l.pos + m.window as u16,
                ..l.clone()
            });
            origin.push(None);
        }
        layers.extend_from_slice(&mv.layers[m.at + k..]);
        origin.extend(m.order[m.at + k..].iter().map(|&i| Some(i)));
        let raw = Word {
            source: mv.source.clone(),
            layers,
        };
        // The moves themselves are not kept: a long chain would hold quadratically many, and the
        // Lean export recomputes the few it needs.
        let c = ctx.canonical_origin(&raw);
        out.push(Summand {
            coeff: rc.clone(),
            sign: c.sign,
            word: c.word,
            origin: c.origin.iter().map(|&j| origin[j]).collect(),
        });
    }
    out
}

/// One step of a rewrite chain: the word `word`, with coefficient `coeff` in the term, rewritten
/// by `rule` at the match `at`.
#[derive(Clone, Debug)]
pub struct Step<R: Ring> {
    pub rule: Rule<R>,
    pub word: Word,
    pub coeff: Poly<R::Elem>,
    pub at: Match,
    pub summands: Vec<Summand<R>>,
}

impl<R: Ring> Step<R> {
    /// What the step adds to the term: `−c·word + Σ ± c·r_j·word_j`.
    pub fn delta(&self, ctx: &Context<R>) -> CTerm<R> {
        let mut v = vec![(self.word.clone(), ctx.coeff.neg(&self.coeff))];
        for s in &self.summands {
            let mut c = ctx.coeff.mul(&self.coeff, &s.coeff);
            if self.at.sign ^ s.sign {
                c = ctx.coeff.neg(&c);
            }
            v.push((s.word.clone(), c));
        }
        ctx.normalize(v)
    }
}

/// A rewrite chain: `start = end`, one step at a time.
#[derive(Clone, Debug)]
pub struct Chain<R: Ring> {
    pub start: CTerm<R>,
    pub steps: Vec<Step<R>>,
    pub end: CTerm<R>,
}

impl<R: Ring> Chain<R> {
    pub fn new(start: CTerm<R>) -> Self {
        Chain {
            end: start.clone(),
            start,
            steps: vec![],
        }
    }

    /// The terms before each step and after the last.
    pub fn terms(&self, ctx: &Context<R>) -> Vec<CTerm<R>> {
        let mut out = vec![self.start.clone()];
        for s in &self.steps {
            let t = ctx.add(out.last().unwrap(), &s.delta(ctx));
            out.push(t);
        }
        out
    }
}

/// Rewrites the word `w` of the chain's current term at the match `m` of `rules[m.rule]`.
pub fn rewrite<R: Ring>(
    ctx: &Context<R>,
    rules: &[Rule<R>],
    chain: &mut Chain<R>,
    w: &Word,
    m: Match,
) -> Result<()> {
    let coeff = coeff_of(&chain.end, w)
        .ok_or_else(|| Error::invalid("the word to rewrite is not in the term"))?;
    let rule = rules[m.rule].clone();
    let summands = apply_match(ctx, &rule, &m);
    let step = Step {
        rule,
        word: w.clone(),
        coeff,
        at: m,
        summands,
    };
    add_into(ctx, &mut chain.end, &step.delta(ctx));
    chain.steps.push(step);
    Ok(())
}

/// `t += d` in place: both are sorted by word with nonzero coefficients, and `d` is short.
fn add_into<R: Ring>(ctx: &Context<R>, t: &mut CTerm<R>, d: &CTerm<R>) {
    for (w, c) in &d.terms {
        match t.terms.binary_search_by(|(v, _)| v.cmp(w)) {
            Ok(i) => {
                let sum = ctx.coeff.add(&t.terms[i].1, c);
                if ctx.coeff.is_zero(&sum) {
                    t.terms.remove(i);
                } else {
                    t.terms[i].1 = sum;
                }
            }
            Err(i) => t.terms.insert(i, (w.clone(), c.clone())),
        }
    }
}

/// Applies the first match of any of `rules` in each word of `t`, the words taken in order.
pub fn apply_everywhere<R: Ring>(
    ctx: &Context<R>,
    rules: &[Rule<R>],
    t: &CTerm<R>,
) -> Result<Chain<R>> {
    let mut chain = Chain::new(t.clone());
    for (w, _) in &t.terms {
        if coeff_of(&chain.end, w).is_none() {
            continue;
        }
        if let Some(m) = find_matches(ctx, rules, w, None).into_iter().next() {
            rewrite(ctx, rules, &mut chain, w, m)?;
        }
    }
    Ok(chain)
}

// ---------------------------------------------------------------------------------------------
// Straightening to the basis `ψ_ŵ x^a` (dots at the bottom) or `x^a ψ_ŵ` (dots on top).

/// The roles of the generators in a basis of crossings and dots.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    pub dot: u16,
    pub crossing: u16,
    pub dots_on_top: bool,
}

/// A move on a word in the crossings, listed top first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Op {
    /// Letters `i, i + 1` on distant strands exchange (free: an interchange).
    Commute(usize),
    /// Letters `i, i + 1, i + 2` read `j k j` with `|j − k| = 1`; they become `k j k`.
    Braid(usize),
    /// Letters `i, i + 1` are equal.
    Square(usize),
}

/// A plan for a word: its crossing layers in list order (top first) and the moves left.
#[derive(Clone, Debug)]
struct Plan {
    labels: Vec<usize>,
    ops: Vec<Op>,
}

fn perm_of(n: usize, top_first: &[usize]) -> Perm {
    let mut p = Perm::identity(n);
    for &k in top_first.iter().rev() {
        p = p.left_mul_simple(k);
    }
    p
}

/// Moves turning the reduced word `a` (top first) of `w` into one starting with `k`, a left
/// descent of `w`, by commutations and braid moves (the exchange property, made constructive).
fn bring_to_front(a: &mut [usize], off: usize, k: usize, ops: &mut Vec<Op>) {
    let j = a[0];
    if j == k {
        return;
    }
    bring_to_front(&mut a[1..], off + 1, k, ops);
    if j.abs_diff(k) >= 2 {
        ops.push(Op::Commute(off));
        a.swap(0, 1);
    } else {
        bring_to_front(&mut a[2..], off + 2, j, ops);
        ops.push(Op::Braid(off));
        a[0] = k;
        a[1] = j;
        a[2] = k;
    }
}

/// The moves from the word `a` (top first, on `n` strands) to the canonical reduced word of its
/// permutation, or, if `a` is not reduced, to the first square from the bottom.
fn plan(n: usize, a: &[usize]) -> Vec<Op> {
    let mut a = a.to_vec();
    let mut ops = Vec::new();
    let r = a.len();
    for j in (0..r.saturating_sub(1)).rev() {
        let below = perm_of(n, &a[j + 1..]);
        if below.has_left_descent(a[j]) {
            let k = a[j];
            bring_to_front(&mut a[j + 1..], j + 1, k, &mut ops);
            ops.push(Op::Square(j));
            return ops;
        }
    }
    let target = perm_of(n, &a).reduced_word();
    for i in 0..r {
        bring_to_front(&mut a[i..], i, target[i], &mut ops);
    }
    ops
}

/// What straightening does next to a word: a match and, for the word the move keeps, the plan
/// that continues it.
fn next_move<R: Ring>(
    ctx: &Context<R>,
    rules: &[Rule<R>],
    layout: &Layout,
    w: &Word,
    planned: Option<&Plan>,
    limit: usize,
) -> Result<Option<(Match, Option<Plan>)>> {
    let graph = Graph::new(ctx, w);
    let is = |i: usize, g: u16| w.layers[i].generator == g;
    // Dots first: a dot next to a crossing on the wrong side moves through it.
    let dots: Vec<usize> = (0..limit).filter(|&i| is(i, layout.dot)).collect();
    let pick = if layout.dots_on_top {
        dots.iter().rev().find_map(|&d| {
            graph.children[d]
                .iter()
                .find(|&&c| c < limit && is(c, layout.crossing))
                .map(|&c| vec![d, c])
        })
    } else {
        dots.iter().find_map(|&d| {
            graph.parents[d]
                .iter()
                .find(|&&c| is(c, layout.crossing))
                .map(|&c| vec![c, d])
        })
    };
    if let Some(set) = pick {
        let m = matches_on(ctx, rules, &graph, w, &set, |rule| {
            let ls = &rule.pattern.layers;
            ls.len() == 2
                && ls[0].generator == set_gen(w, set[0])
                && ls[1].generator == set_gen(w, set[1])
        });
        return match m {
            Some(m) => Ok(Some((m, None))),
            None => Err(Error::precondition(format!(
                "no relation moves the dot at layer {} through the crossing at layer {}",
                if layout.dots_on_top { set[0] } else { set[1] } + 1,
                if layout.dots_on_top { set[1] } else { set[0] } + 1,
            ))),
        };
    }
    // Then the crossings: towards the canonical reduced word, or to a square.
    let mut p = match planned {
        Some(p) => p.clone(),
        None => {
            let labels: Vec<usize> = (0..limit)
                .rev()
                .filter(|&i| is(i, layout.crossing))
                .collect();
            let letters: Vec<usize> = labels.iter().map(|&i| w.layers[i].pos as usize).collect();
            Plan {
                ops: plan(w.source.len(), &letters),
                labels,
            }
        }
    };
    while let Some(&Op::Commute(i)) = p.ops.first() {
        p.labels.swap(i, i + 1);
        p.ops.remove(0);
    }
    let Some(&op) = p.ops.first() else {
        return Ok(None);
    };
    p.ops.remove(0);
    let (i, len) = match op {
        Op::Braid(i) => (i, 3),
        Op::Square(i) => (i, 2),
        Op::Commute(_) => unreachable!(),
    };
    let mut set: Vec<usize> = p.labels[i..i + len].to_vec();
    set.sort_unstable();
    let m = matches_on(ctx, rules, &graph, w, &set, |rule| {
        rule.pattern.layers.len() == len
            && rule
                .pattern
                .layers
                .iter()
                .all(|l| l.generator == layout.crossing)
    })
    .ok_or_else(|| {
        Error::precondition(format!(
            "no relation {} the crossings at layers {}",
            if len == 3 {
                "braids"
            } else {
                "removes the double crossing of"
            },
            set.iter()
                .map(|x| (x + 1).to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ))
    })?;
    // The braided letters get the new crossings' layers once the step is applied.
    let next = (len == 3).then(|| {
        for l in &mut p.labels[i..i + 3] {
            *l = usize::MAX;
        }
        p
    });
    Ok(Some((m, next)))
}

/// The next move on the lowest layers of `w` that are not yet in normal form: straightening
/// works upwards, multiplying a normal form by one more layer at a time, so intermediate terms
/// stay close to normal forms. A word in the middle of a planned braid sequence keeps its plan.
fn first_move<R: Ring>(
    ctx: &Context<R>,
    rules: &[Rule<R>],
    layout: &Layout,
    w: &Word,
    plans: &HashMap<Word, Plan>,
) -> Result<Option<(usize, Move)>> {
    let len = w.layers.len();
    if let Some(p) = plans.get(w) {
        let limit = p.labels.iter().max().map_or(len, |&m| m + 1);
        if let Some(m) = next_move(ctx, rules, layout, w, Some(p), limit)? {
            return Ok(Some((len - limit, m)));
        }
    }
    let Some(full) = next_move(ctx, rules, layout, w, None, len)? else {
        return Ok(None);
    };
    // The smallest prefix with a move, by bisection (a prefix with a move almost always
    // stays one when layers are added; when not, the bisection still ends on a prefix that has
    // one).
    let (mut lo, mut hi, mut best) = (0, len, full);
    while hi - lo > 1 {
        let mid = (lo + hi) / 2;
        match next_move(ctx, rules, layout, w, None, mid)? {
            Some(m) => {
                hi = mid;
                best = m;
            }
            None => lo = mid,
        }
    }
    Ok(Some((len - hi, best)))
}

/// A move: the match to rewrite, and the plan its main result continues with.
type Move = (Match, Option<Plan>);

fn set_gen(w: &Word, i: usize) -> u16 {
    w.layers[i].generator
}

/// The first match, among the rules passing `filter`, whose occurrence is exactly `set`.
fn matches_on<R: Ring>(
    ctx: &Context<R>,
    rules: &[Rule<R>],
    graph: &Graph,
    w: &Word,
    set: &[usize],
    filter: impl Fn(&Rule<R>) -> bool,
) -> Option<Match> {
    let mut sorted = set.to_vec();
    sorted.sort_unstable();
    for (ri, rule) in rules.iter().enumerate() {
        if !filter(rule) {
            continue;
        }
        let mut labels: Vec<Label> = rule.pattern.layers.iter().map(label).collect();
        let mut have: Vec<Label> = sorted.iter().map(|&i| label(&w.layers[i])).collect();
        labels.sort();
        have.sort();
        if labels != have {
            continue;
        }
        if let Some(m) = match_set(ctx, graph, ri, rule, w, &sorted)
            .into_iter()
            .next()
        {
            return Some(m);
        }
    }
    None
}

/// The first match of any rule whose occurrence is exactly the layers `set` of `w`.
pub fn match_on<R: Ring>(
    ctx: &Context<R>,
    rules: &[Rule<R>],
    w: &Word,
    set: &[usize],
) -> Option<Match> {
    let graph = Graph::new(ctx, w);
    matches_on(ctx, rules, &graph, w, set, |_| true)
}

/// The layer directly above (or below) the one-strand layer `i` of `w` on its strand.
pub fn neighbour<R: Ring>(ctx: &Context<R>, w: &Word, i: usize, up: bool) -> Option<usize> {
    let graph = Graph::new(ctx, w);
    let next = if up {
        &graph.children[i]
    } else {
        &graph.parents[i]
    };
    next.first().copied()
}

/// Rewrites `t` to a combination of basis words `ψ_ŵ x^a` (or `x^a ψ_ŵ`), recording the chain.
///
/// Dots move through crossings by the dot-slide relations; then each word of crossings is
/// brought to the canonical reduced word of its permutation by braid moves, or, when it is not
/// reduced, to a double crossing that the quadratic relation removes. The moves are planned with
/// the exchange property, so the chain is finite; `max_steps` caps it.
pub fn straighten<R: Ring>(
    ctx: &Context<R>,
    rules: &[Rule<R>],
    layout: &Layout,
    t: &CTerm<R>,
    max_steps: usize,
) -> Result<Chain<R>> {
    straighten_impl(ctx, rules, layout, t, max_steps, true).map(|(chain, _)| chain)
}

/// The normal form [`straighten`] reaches and the number of steps it takes, without keeping the
/// steps: a chain of a long word holds every intermediate word, which this avoids.
pub fn normal_form<R: Ring>(
    ctx: &Context<R>,
    rules: &[Rule<R>],
    layout: &Layout,
    t: &CTerm<R>,
    max_steps: usize,
) -> Result<(CTerm<R>, usize)> {
    straighten_impl(ctx, rules, layout, t, max_steps, false).map(|(chain, n)| (chain.end, n))
}

fn straighten_impl<R: Ring>(
    ctx: &Context<R>,
    rules: &[Rule<R>],
    layout: &Layout,
    t: &CTerm<R>,
    max_steps: usize,
    keep: bool,
) -> Result<(Chain<R>, usize)> {
    let mut chain = Chain::new(t.clone());
    let mut count = 0;
    let mut plans: HashMap<Word, Plan> = HashMap::new();
    // The next move of each word of the current term, and how many layers lie above the part
    // it works on. The word with the most layers still to come goes first, so that the whole
    // term advances one layer at a time and equal words meet, and cancel, early.
    // Only the words a step touches change, so both are updated word by word.
    let mut moves: HashMap<Word, Option<(usize, Move)>> = HashMap::new();
    let mut queue: BTreeSet<(usize, Word)> = BTreeSet::new();
    let present = |t: &CTerm<R>, w: &Word| t.terms.binary_search_by(|(v, _)| v.cmp(w)).is_ok();
    let refresh = |w: &Word,
                   end: &CTerm<R>,
                   plans: &HashMap<Word, Plan>,
                   moves: &mut HashMap<Word, Option<(usize, Move)>>,
                   queue: &mut BTreeSet<(usize, Word)>|
     -> Result<()> {
        if !present(end, w) {
            if let Some(Some((above, _))) = moves.remove(w) {
                queue.remove(&(above, w.clone()));
            }
            return Ok(());
        }
        if !moves.contains_key(w) {
            let mv = first_move(ctx, rules, layout, w, plans)?;
            if let Some((above, _)) = &mv {
                queue.insert((*above, w.clone()));
            }
            moves.insert(w.clone(), mv);
        }
        Ok(())
    };
    for (w, _) in &t.terms {
        refresh(w, &chain.end, &plans, &mut moves, &mut queue)?;
    }
    while let Some((above, w)) = queue.pop_last() {
        let (_, (m, next)) = moves.remove(&w).flatten().expect("a queued move");
        debug_assert!(above <= w.layers.len());
        if count >= max_steps {
            return Err(Error::precondition(format!(
                "straightening stopped at the cap of {max_steps} steps"
            )));
        }
        count += 1;
        rewrite(ctx, rules, &mut chain, &w, m)?;
        if let Some(mut q) = next {
            let step = chain.steps.last().unwrap();
            let crossings = |x: &Word| {
                x.layers
                    .iter()
                    .filter(|l| l.generator == layout.crossing)
                    .count()
            };
            let main = step
                .summands
                .iter()
                .find(|s| crossings(&s.word) == crossings(&step.word));
            if let Some(s) = main {
                let mut created: Vec<usize> = (0..s.word.layers.len())
                    .filter(|&j| {
                        s.origin[j].is_none() && s.word.layers[j].generator == layout.crossing
                    })
                    .collect();
                created.reverse();
                let mut fresh = created.into_iter();
                for l in q.labels.iter_mut() {
                    *l = if *l == usize::MAX {
                        fresh.next().expect("three new crossings")
                    } else {
                        s.origin
                            .iter()
                            .position(|&o| o == Some(*l))
                            .expect("kept crossing")
                    };
                }
                if let Some(Some((above, _))) = moves.remove(&s.word) {
                    queue.remove(&(above, s.word.clone()));
                }
                plans.insert(s.word.clone(), q);
            }
        }
        let step = chain.steps.last().unwrap();
        let touched: Vec<Word> = std::iter::once(step.word.clone())
            .chain(step.summands.iter().map(|s| s.word.clone()))
            .collect();
        for x in &touched {
            refresh(x, &chain.end, &plans, &mut moves, &mut queue)?;
        }
        if !keep {
            chain.steps.clear();
        }
    }
    Ok((chain, count))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plans_reach_the_canonical_word() {
        // s_1 s_2 s_1 (top first, 0-based letters 0 1 0) -> canonical reduced word.
        let n = 3;
        for a in [vec![0, 1, 0], vec![1, 0, 1], vec![2, 0], vec![0, 2, 1, 0]] {
            let n = if a.contains(&2) { 4 } else { n };
            let mut cur = a.clone();
            for op in plan(n, &a) {
                match op {
                    Op::Commute(i) => cur.swap(i, i + 1),
                    Op::Braid(i) => {
                        let (j, k) = (cur[i], cur[i + 1]);
                        cur[i] = k;
                        cur[i + 1] = j;
                        cur[i + 2] = k;
                    }
                    Op::Square(_) => panic!("reduced"),
                }
            }
            assert_eq!(cur, perm_of(n, &a).reduced_word(), "from {a:?}");
        }
        assert_eq!(plan(3, &[0, 1, 1]).last(), Some(&Op::Square(1)));
        let ops = plan(3, &[0, 1, 0, 1]);
        assert!(matches!(ops.last(), Some(Op::Square(_))));
    }
}
