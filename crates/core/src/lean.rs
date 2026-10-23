//! Export of rewrite chains as Lean proofs.
//!
//! A presentation's `[lean]` table names the Lean library its diagrams live in, the Lean shape of
//! each generator and the lemma proving each relation. The exporter writes one theorem per
//! chain, stating `start = end` over an arbitrary Cartan datum whose entries agree with the
//! context's wherever the chain uses them, and proving it step by step: the interchange moves
//! of each step are `dswap` moves, the rewrite is `dg_stepL_at` with the relation's lemma placed
//! between the strands left and right of the rectangle, and the results return to canonical
//! layering by further `dswap` moves. The encoding supported is `kl3-dg`, the normal-form
//! diagrams `dg` of categorification-lean's `Categorification.Diagrams.KL3`.

use std::collections::BTreeSet;

use num_bigint::BigInt;

use crate::context::Context;
use crate::elab::CTerm;
use crate::error::{Error, Result};
use crate::pres::LeanDecl;
use crate::rewrite::{Chain, Rule};
use crate::ring::Ring;
use crate::term::{Letter, Object, Word};

/// A theorem to export: `lhs = rhs`, each side with the chain that straightens it; both chains
/// must end at the same term. With one chain, the statement is that chain's `start = end`.
pub struct Item<'a, R: Ring> {
    pub name: String,
    pub doc: String,
    pub chains: Vec<&'a Chain<R>>,
}

/// A Lean file with one theorem per item.
pub fn export_file<R: Ring>(ctx: &Context<R>, items: &[Item<R>], header: &str) -> Result<String> {
    let decl = lean_decl(ctx)?;
    let mut o = String::new();
    o.push_str(header);
    for m in decl.module.split_whitespace() {
        o.push_str(&format!("import {m}\n"));
    }
    o.push('\n');
    o.push_str("set_option backward.isDefEq.respectTransparency false\n");
    o.push_str("set_option linter.unusedVariables false\n");
    o.push_str("set_option linter.unusedSimpArgs false\n");
    o.push_str("set_option linter.unusedTactic false\n");
    o.push_str("set_option linter.unreachableTactic false\n\n");
    o.push_str("noncomputable section\n\n");
    o.push_str(&format!("namespace {}\n\n", decl.namespace));
    if !decl.open.is_empty() {
        o.push_str(&format!("open {}\n\n", decl.open));
    }
    o.push_str("universe w u v\n\n");
    for it in items {
        o.push_str(&theorem(ctx, decl, it)?);
        o.push('\n');
    }
    o.push_str(&format!("end {}\n", decl.namespace));
    Ok(o)
}

fn lean_decl<R: Ring>(ctx: &Context<R>) -> Result<&LeanDecl> {
    let decl = ctx.pres.lean.as_ref().ok_or_else(|| {
        Error::precondition(format!(
            "presentation {} has no [lean] table: its relations have no Lean counterpart",
            ctx.pres.name
        ))
    })?;
    if decl.encoding != "kl3-dg" {
        return Err(Error::precondition(format!(
            "the Lean encoding '{}' is not supported (only kl3-dg)",
            decl.encoding
        )));
    }
    if ctx.base.characteristic() != 0 {
        return Err(Error::precondition(
            "chains are exported over every commutative ring, so the context's ground ring must \
             have characteristic 0 and the coefficients must be integers",
        ));
    }
    Ok(decl)
}

struct Writer<'c, R: Ring> {
    ctx: &'c Context<R>,
    decl: &'c LeanDecl,
}

impl<R: Ring> Writer<'_, R> {
    fn colour(&self, c: u16) -> String {
        format!("c{}", self.ctx.colours[c as usize].name)
    }

    fn letter(&self, l: &Letter) -> String {
        format!(
            "{} {}",
            if l.up { "up" } else { "dn" },
            self.colour(l.colour)
        )
    }

    fn object(&self, o: &[Letter]) -> String {
        let parts: Vec<String> = o.iter().map(|l| self.letter(l)).collect();
        format!("[{}]", parts.join(", "))
    }

    fn word(&self, w: &Word, weight: &str) -> Result<String> {
        let mut obj: Object = w.source.clone();
        let mut layers = Vec::new();
        for l in &w.layers {
            let g = &self.ctx.pres.generators[l.generator as usize];
            let shape = self.decl.generators.get(&g.name).ok_or_else(|| {
                Error::precondition(format!("generator {} has no Lean shape", g.name))
            })?;
            let mut shape = shape.clone();
            for (v, &c) in g.vars.iter().zip(&l.binding) {
                shape = shape.replace(&format!("{{{v}}}"), &self.colour(c));
            }
            let p = l.pos as usize;
            let s = self.ctx.source_len(l.generator as usize);
            layers.push(format!(
                "({}, {}, {})",
                self.object(&obj[..p]),
                shape,
                self.object(&obj[p + s..])
            ));
            obj = self.ctx.apply(l, &obj);
        }
        Ok(format!(
            "dg RD k {weight} {} {} [{}]",
            self.object(&w.source),
            self.object(&obj),
            layers.join(", ")
        ))
    }

    fn int(&self, c: &crate::poly::Poly<R::Elem>) -> Result<BigInt> {
        let coeff = &self.ctx.coeff;
        if !coeff.is_constant(c) {
            return Err(Error::precondition(
                "a coefficient involves the context's scalars; only integer coefficients export",
            ));
        }
        let s = coeff.base.fmt_elem(&coeff.constant_term(c));
        s.parse::<BigInt>()
            .map_err(|_| Error::precondition(format!("the coefficient {s} is not an integer")))
    }

    /// `Σ c_w • D(w)` with integer coefficients.
    fn term(&self, t: &CTerm<R>, weight: &str) -> Result<String> {
        if t.terms.is_empty() {
            return Ok("0".into());
        }
        let mut s = String::new();
        for (k, (w, c)) in t.terms.iter().enumerate() {
            let n = self.int(c)?;
            let neg = n < BigInt::from(0);
            let a = if neg { -n } else { n };
            if k == 0 {
                if neg {
                    s.push('-');
                }
            } else {
                s.push_str(if neg { " - " } else { " + " });
            }
            let d = self.word(w, weight)?;
            if a == BigInt::from(1) {
                s.push_str(&d);
            } else {
                s.push_str(&format!("({a} : ℤ) • {d}"));
            }
        }
        Ok(s)
    }
}

fn distinct(a: u16, b: u16) -> (u16, u16) {
    (a.min(b), a.max(b))
}

/// Cartan data a lemma depends on, with the context's value.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Fact {
    /// `C.dot a b = 0` (false) or `≠ 0` (true).
    Dot(u16, u16, bool),
    /// `C.dij a b = n`.
    Dij(u16, u16, i64),
    /// `A C a b = n`, for chains through weight-dependent relations.
    A(u16, u16, i64),
    /// `ip RD a μ = n`: the weight of the rightmost region.
    Ip(u16, i64),
}

fn word_colours(w: &Word, out: &mut BTreeSet<u16>) {
    out.extend(w.source.iter().map(|l| l.colour));
    for l in &w.layers {
        out.extend(l.binding.iter().copied());
    }
}

fn theorem<R: Ring>(ctx: &Context<R>, decl: &LeanDecl, it: &Item<R>) -> Result<String> {
    let wr = Writer { ctx, decl };
    let cartan = ctx
        .cartan
        .as_ref()
        .ok_or_else(|| Error::precondition("the kl3-dg encoding needs a Cartan datum"))?;
    if it.chains.is_empty() || it.chains.len() > 2 {
        return Err(Error::invalid("a theorem needs one or two chains"));
    }
    if it.chains.len() == 2 && it.chains[0].end != it.chains[1].end {
        return Err(Error::invalid(
            "the two chains end at different terms: the sides are not shown equal",
        ));
    }
    // Colours, facts and lemmas the chains use.
    let mut colours: BTreeSet<u16> = BTreeSet::new();
    let mut facts: BTreeSet<Fact> = BTreeSet::new();
    let mut rules: Vec<&Rule<R>> = Vec::new();
    for ch in &it.chains {
        for (w, _) in ch.start.terms.iter().chain(&ch.end.terms) {
            word_colours(w, &mut colours);
        }
        for st in &ch.steps {
            word_colours(&st.word, &mut colours);
            colours.extend(st.rule.binding.iter().map(|&c| c as u16));
            if st.at.sign || st.summands.iter().any(|s| s.sign) {
                return Err(Error::precondition(
                    "the chain uses Koszul signs, which the kl3-dg encoding does not have",
                ));
            }
            let rel = decl.relations.get(&st.rule.relation).ok_or_else(|| {
                Error::precondition(format!(
                    "the relation {} has no Lean lemma in the [lean] table",
                    st.rule.relation
                ))
            })?;
            let names = &ctx
                .pres
                .relations
                .iter()
                .find(|r| r.name == st.rule.relation)
                .expect("relation")
                .vars;
            for f in &rel.facts {
                let parts: Vec<&str> = f.split_whitespace().collect();
                let col = |p: &str| -> Result<u16> {
                    let v = p.trim_start_matches('{').trim_end_matches('}');
                    let i = names
                        .iter()
                        .position(|n| n == v)
                        .ok_or_else(|| Error::invalid(format!("fact '{f}': unknown '{p}'")))?;
                    Ok(st.rule.binding[i] as u16)
                };
                match parts.as_slice() {
                    ["dot", a, b] => {
                        let (a, b) = (col(a)?, col(b)?);
                        facts.insert(Fact::Dot(a, b, cartan.form(a as usize, b as usize) != 0));
                    }
                    ["dot≠0", a, b] => {
                        let (a, b) = (col(a)?, col(b)?);
                        if cartan.form(a as usize, b as usize) == 0 {
                            return Err(Error::precondition(format!(
                                "the Lean lemma for {} needs (α_{}, α_{}) ≠ 0",
                                st.rule.relation,
                                ctx.colours[a as usize].name,
                                ctx.colours[b as usize].name
                            )));
                        }
                        facts.insert(Fact::Dot(a, b, true));
                    }
                    ["dij", a, b] => {
                        let (a, b) = (col(a)?, col(b)?);
                        if a == b {
                            return Err(Error::invalid(format!("fact '{f}' needs two colours")));
                        }
                        facts.insert(Fact::Dij(a, b, -cartan.a[a as usize][b as usize]));
                    }
                    _ => return Err(Error::invalid(format!("unknown Lean fact '{f}'"))),
                }
            }
            if !rules.iter().any(|r| same_rule(r, &st.rule)) {
                rules.push(&st.rule);
            }
        }
    }
    if rules.iter().any(|r| r.region.is_some()) {
        let mu = ctx.mu.as_ref().ok_or_else(|| {
            Error::precondition("a weight-dependent relation needs the context's weight mu")
        })?;
        for &a in &colours {
            facts.insert(Fact::Ip(a, mu[a as usize]));
            for &b in &colours {
                if a != b {
                    facts.insert(Fact::A(a, b, cartan.a[a as usize][b as usize]));
                }
            }
        }
    }
    let ne_name = |a: u16, b: u16| {
        let (x, y) = distinct(a, b);
        format!("h_{}_{}", wr.colour(x), wr.colour(y))
    };
    let pairs: Vec<(u16, u16)> = colours
        .iter()
        .flat_map(|&a| {
            colours
                .iter()
                .filter(move |&&b| b > a)
                .map(move |&b| (a, b))
        })
        .collect();
    // Statement.
    let mut o = String::new();
    if !it.doc.is_empty() {
        o.push_str(&format!("/-- {} -/\n", it.doc));
    }
    o.push_str(&format!(
        "theorem {} {{I : Type u}} {{C : CartanDatum I}} {{X Y : Type v}} [AddCommGroup X]\n    \
         [AddCommGroup Y] (RD : WeightDatum C X Y) (k : Type w) [CommRing k] (μ : X)",
        it.name
    ));
    let cs: Vec<String> = colours.iter().map(|&c| wr.colour(c)).collect();
    o.push_str(&format!("\n    ({} : I)", cs.join(" ")));
    for &(a, b) in &pairs {
        o.push_str(&format!(
            "\n    ({} : {} ≠ {})",
            ne_name(a, b),
            wr.colour(a),
            wr.colour(b)
        ));
    }
    let fact_names: Vec<String> = facts
        .iter()
        .map(|f| match f {
            Fact::Dot(a, b, _) => format!("hdot_{}_{}", wr.colour(*a), wr.colour(*b)),
            Fact::Dij(a, b, _) => format!("hdij_{}_{}", wr.colour(*a), wr.colour(*b)),
            Fact::A(a, b, _) => format!("hA_{}_{}", wr.colour(*a), wr.colour(*b)),
            Fact::Ip(a, _) => format!("hμ_{}", wr.colour(*a)),
        })
        .collect();
    for (f, n) in facts.iter().zip(&fact_names) {
        let s = match f {
            Fact::Dot(a, b, nz) => format!(
                "C.dot {} {} {} 0",
                wr.colour(*a),
                wr.colour(*b),
                if *nz { "≠" } else { "=" }
            ),
            Fact::Dij(a, b, v) => format!("C.dij {} {} = {v}", wr.colour(*a), wr.colour(*b)),
            Fact::A(a, b, v) => format!("A C {} {} = {v}", wr.colour(*a), wr.colour(*b)),
            Fact::Ip(a, v) => format!("ip RD {} μ = {v}", wr.colour(*a)),
        };
        o.push_str(&format!("\n    ({n} : {s})"));
    }
    let lhs = &it.chains[0].start;
    let rhs = if it.chains.len() == 2 {
        &it.chains[1].start
    } else {
        &it.chains[0].end
    };
    o.push_str(&format!(
        " :\n    {} =\n      {} := by\n",
        wr.term(lhs, "μ")?,
        wr.term(rhs, "μ")?
    ));
    // The relation instances, at every weight.
    let simp_hyps: Vec<String> = pairs
        .iter()
        .flat_map(|&(a, b)| [ne_name(a, b), format!("({}).symm", ne_name(a, b))])
        .collect();
    for (ri, r) in rules.iter().enumerate() {
        let rel = &decl.relations[&r.relation];
        let prel = ctx
            .pres
            .relations
            .iter()
            .find(|x| x.name == r.relation)
            .expect("relation");
        let names = &prel.vars;
        let mut lemma = rel.lemma.clone();
        // The weight hypotheses `ip RD c ν = n` of a weight-dependent rule, one per colour.
        let mut weight_hyps: Vec<(u16, i64)> = Vec::new();
        if let Some(region) = &r.region {
            for &c in &r.binding {
                let c = c as u16;
                if !weight_hyps.iter().any(|h| h.0 == c) {
                    weight_hyps.push((c, region[c as usize]));
                }
            }
        }
        for (i, v) in names.iter().enumerate() {
            lemma = lemma.replace(
                &format!("{{ip {v}}}"),
                &format!("hν_{}", wr.colour(r.binding[i] as u16)),
            );
        }
        for ((v, _, _), n) in prel.ints.iter().zip(&r.ints) {
            lemma = lemma.replace(&format!("{{{v}}}"), &n.to_string());
        }
        for (i, v) in names.iter().enumerate() {
            for (j, u) in names.iter().enumerate() {
                let (a, b) = (r.binding[i] as u16, r.binding[j] as u16);
                if a != b {
                    let pf = if a < b {
                        ne_name(a, b)
                    } else {
                        format!("({}).symm", ne_name(a, b))
                    };
                    lemma = lemma.replace(&format!("{{{v}!={u}}}"), &pf);
                }
            }
        }
        for (i, v) in names.iter().enumerate() {
            lemma = lemma.replace(&format!("{{{v}}}"), &wr.colour(r.binding[i] as u16));
        }
        lemma = lemma.replace(
            "{simp}",
            &if simp_hyps.is_empty() {
                "(by simp)".to_string()
            } else {
                format!("(by simp [{}])", simp_hyps.join(", "))
            },
        );
        if lemma.contains('{') {
            return Err(Error::invalid(format!(
                "Lean lemma template for {} has an unknown placeholder: {lemma}",
                r.relation
            )));
        }
        o.push_str(&format!(
            "  -- {} ({}){}\n",
            r.relation,
            if r.forward {
                "left to right"
            } else {
                "right to left"
            },
            binding_note(ctx, names, &r.binding)
        ));
        let premises: String = weight_hyps
            .iter()
            .map(|(c, n)| format!("ip RD {} ν = {n} → ", wr.colour(*c)))
            .collect();
        let binders: String = weight_hyps
            .iter()
            .map(|(c, _)| format!(" hν_{}", wr.colour(*c)))
            .collect();
        o.push_str(&format!(
            "  have E{ri} : ∀ ν : X, {premises}{} =\n      {} := fun ν{binders} => by\n",
            wr.word(&r.pattern, "ν")?,
            wr.term(&relaid(ctx, &rel.layering, &r.replacement)?, "ν")?
        ));
        match rel.proof.as_str() {
            "exact" => {
                o.push_str(&format!("    exact {lemma}\n"));
                continue;
            }
            "tactic" => {
                for line in lemma.split("; ") {
                    o.push_str(&format!("    {line}\n"));
                }
                continue;
            }
            _ => {}
        }
        o.push_str(&format!("    rw [{lemma}]\n"));
        if !rel.facts.is_empty() {
            o.push_str(&format!(
                "    try simp only [{}, ↓reduceIte, Finset.sum_range_succ, Finset.sum_range_zero, \
                 zero_add, Nat.reduceSub, Nat.reduceAdd, Nat.sub_zero, List.replicate_succ, \
                 List.replicate_zero, List.cons_append, List.nil_append, List.append_nil]\n",
                fact_names.join(", ")
            ));
        }
        o.push_str("    try abel\n");
    }
    // The steps.
    let side_names = ["L", "R"];
    for (ci, ch) in it.chains.iter().enumerate() {
        let tag = if it.chains.len() == 2 {
            side_names[ci]
        } else {
            ""
        };
        let mut rewrites = Vec::new();
        for (si, st) in ch.steps.iter().enumerate() {
            let ri = rules
                .iter()
                .position(|r| same_rule(r, &st.rule))
                .expect("rule");
            let m = &st.at;
            // The object below the rectangle, split around it.
            let mut obj = m.moved.source.clone();
            for l in &m.moved.layers[..m.at] {
                obj = ctx.apply(l, &obj);
            }
            let width = st.rule.pattern.source.len();
            let u = wr.object(&obj[..m.window]);
            let v = wr.object(&obj[m.window + width..]);
            // The results as the lemma lays them out, and the moves to canonical layering.
            let layering = &decl.relations[&st.rule.relation].layering;
            let rep = relaid(ctx, layering, &st.rule.replacement)?;
            let mut raws = Vec::new();
            for ((rw, rc), s) in rep.terms.iter().zip(&st.summands) {
                let mut layers = m.moved.layers[..m.at].to_vec();
                layers.extend(rw.layers.iter().map(|l| crate::term::Layer {
                    pos: l.pos + m.window as u16,
                    ..l.clone()
                }));
                layers.extend_from_slice(&m.moved.layers[m.at + st.rule.pattern.layers.len()..]);
                let raw = Word {
                    source: m.moved.source.clone(),
                    layers,
                };
                let c = ctx.canonical_tracked(&raw);
                if c.word != s.word || c.sign {
                    return Err(Error::invalid(
                        "internal: the relaid result is not the step's result",
                    ));
                }
                raws.push((raw, rc.clone(), c.swaps, s.word.clone()));
            }
            let raw: CTerm<R> = crate::term::Term {
                terms: raws
                    .iter()
                    .map(|(w, c, _, _)| (w.clone(), c.clone()))
                    .collect(),
            };
            let kn = format!("K{tag}{si}");
            let place = match width {
                0 => format!("left of strand {}", m.window + 1),
                1 => format!("strand {}", m.window + 1),
                _ => format!("strands {}–{}", m.window + 1, m.window + width),
            };
            o.push_str(&format!(
                "  -- step {}: {} at layer {}, {place}\n",
                si + 1,
                st.rule.relation,
                m.at + 1,
            ));
            o.push_str(&format!(
                "  have {kn} : {} =\n      {} := by\n",
                wr.word(&st.word, "μ")?,
                wr.term(&raw, "μ")?
            ));
            for s in &m.swaps {
                o.push_str(&format!("    dswap {s}\n"));
            }
            // Proofs of the rule's weight hypotheses at the region right of the rectangle.
            let mut weight_args = String::new();
            if st.rule.region.is_some() {
                let mut seen = Vec::new();
                for &c in &st.rule.binding {
                    let c = c as u16;
                    if seen.contains(&c) {
                        continue;
                    }
                    seen.push(c);
                    let hs: Vec<String> = colours
                        .iter()
                        .filter(|&&d| d != c)
                        .map(|&d| format!("hA_{}_{}", wr.colour(c), wr.colour(d)))
                        .collect();
                    let hc = format!("hμ_{}", wr.colour(c));
                    weight_args.push_str(&format!(
                        " (by simp only [ip, wt_cons, wt_nil, sh, map_add, map_zsmul, \
                         RD.pair_iY_iX_eq_A, smul_eq_mul] at {hc} ⊢; simp [sgn, A_self{}, {hc}])",
                        hs.iter().map(|h| format!(", {h}")).collect::<String>()
                    ));
                }
            }
            o.push_str(&format!(
                "    rw [dg_stepL_at _ {} {u} {v} (E{ri} _{weight_args}) (by (try dnorm); schain) \
                 (by (try dnorm); schain) (by dnorm)]\n",
                m.at
            ));
            o.push_str("    try simp only [map_add, map_sub, map_neg, map_zsmul, map_zero]\n");
            o.push_str(
                "    repeat rw [ctxL_dg RD k _ (by (try dnorm); schain) (by (try dnorm); schain)]\n",
            );
            o.push_str("    try dnorm\n    try abel\n");
            rewrites.push(kn);
            let mut seen: BTreeSet<Word> = BTreeSet::new();
            for (j, (raw, _, swaps, word)) in raws.iter().enumerate() {
                if swaps.is_empty() || !seen.insert(raw.clone()) {
                    continue;
                }
                let cn = format!("C{tag}{si}_{j}");
                o.push_str(&format!(
                    "  have {cn} : {} =\n      {} := by\n",
                    wr.word(raw, "μ")?,
                    wr.word(word, "μ")?
                ));
                for sw in swaps {
                    o.push_str(&format!("    dswap {sw}\n"));
                }
                o.push_str("    try rfl\n");
                rewrites.push(cn);
            }
        }
        let target = if it.chains.len() == 2 {
            format!("{} = {}", wr.term(&ch.start, "μ")?, wr.term(&ch.end, "μ")?)
        } else {
            String::new()
        };
        if it.chains.len() == 2 {
            o.push_str(&format!("  have {tag} : {target} := by\n"));
            push_rewrites(&mut o, &rewrites, "    ");
        } else {
            push_rewrites(&mut o, &rewrites, "  ");
        }
    }
    if it.chains.len() == 2 {
        o.push_str("  exact L.trans R.symm\n");
    }
    Ok(o)
}

/// The replacement with each word laid out as the Lean lemma writes it.
fn relaid<R: Ring>(ctx: &Context<R>, layering: &str, t: &CTerm<R>) -> Result<CTerm<R>> {
    let mut terms = Vec::new();
    for (w, c) in &t.terms {
        let w = match layering {
            "left-first" => {
                // Interchange moves only: generators on strands further left go lower, as far as
                // they can.
                let mut w = w.clone();
                loop {
                    let mut moved = false;
                    for i in 0..w.layers.len().saturating_sub(1) {
                        if w.layers[i].pos <= w.layers[i + 1].pos {
                            continue;
                        }
                        if let Some((lo, up, odd)) =
                            ctx.swap_adjacent(&w.layers[i], &w.layers[i + 1])
                        {
                            if odd {
                                return Err(Error::precondition(
                                    "relaying a word costs a Koszul sign",
                                ));
                            }
                            w.layers[i] = lo;
                            w.layers[i + 1] = up;
                            moved = true;
                        }
                    }
                    if !moved {
                        break w;
                    }
                }
            }
            "bubbles-first" => {
                // Interchange moves only: closed components (bubbles) go lower, as far as they
                // can, past the other generators.
                let mut w = w.clone();
                let mut bub = bubble_layers(ctx, &w);
                loop {
                    let mut moved = false;
                    for i in 0..w.layers.len().saturating_sub(1) {
                        if bub[i] || !bub[i + 1] {
                            continue;
                        }
                        if let Some((lo, up, odd)) =
                            ctx.swap_adjacent(&w.layers[i], &w.layers[i + 1])
                        {
                            if odd {
                                return Err(Error::precondition(
                                    "relaying a word costs a Koszul sign",
                                ));
                            }
                            w.layers[i] = lo;
                            w.layers[i + 1] = up;
                            bub.swap(i, i + 1);
                            moved = true;
                        }
                    }
                    if !moved {
                        break w;
                    }
                }
            }
            _ => w.clone(),
        };
        terms.push((w, c.clone()));
    }
    Ok(crate::term::Term { terms })
}

/// For each layer of `w`, whether it belongs to a closed component: one that meets neither the
/// source nor the target, such as a bubble with its dots. Every generator joins its ends.
fn bubble_layers<R: Ring>(ctx: &Context<R>, w: &Word) -> Vec<bool> {
    fn find(p: &mut [usize], mut x: usize) -> usize {
        while p[x] != x {
            p[x] = p[p[x]];
            x = p[x];
        }
        x
    }
    let mut parent: Vec<usize> = (0..w.source.len()).collect();
    let mut cur: Vec<usize> = (0..w.source.len()).collect();
    let mut open: Vec<usize> = cur.clone();
    let mut ends: Vec<usize> = Vec::new();
    for l in &w.layers {
        let g = l.generator as usize;
        let (sl, tl) = (ctx.source_len(g), ctx.target_len(g));
        let p = l.pos as usize;
        let ins: Vec<usize> = cur[p..p + sl].to_vec();
        let outs: Vec<usize> = (0..tl)
            .map(|_| {
                parent.push(parent.len());
                parent.len() - 1
            })
            .collect();
        let all: Vec<usize> = ins.iter().chain(&outs).copied().collect();
        if let Some(&first) = all.first() {
            for &x in &all[1..] {
                let (a, b) = (find(&mut parent, first), find(&mut parent, x));
                parent[b] = a;
            }
        }
        ends.push(all.first().copied().unwrap_or(usize::MAX));
        cur.splice(p..p + sl, outs);
    }
    open.extend(cur.iter().copied());
    let open_roots: Vec<usize> = open.iter().map(|&x| find(&mut parent, x)).collect();
    ends.iter()
        .map(|&e| e != usize::MAX && !open_roots.contains(&find(&mut parent, e)))
        .collect()
}

fn push_rewrites(o: &mut String, rewrites: &[String], indent: &str) {
    for chunk in rewrites.chunks(12) {
        o.push_str(&format!("{indent}rw [{}]\n", chunk.join(", ")));
    }
    o.push_str(&format!("{indent}try abel\n"));
}

fn same_rule<R: Ring>(a: &Rule<R>, b: &Rule<R>) -> bool {
    a.relation == b.relation
        && a.binding == b.binding
        && a.ints == b.ints
        && a.region == b.region
        && a.forward == b.forward
}

fn binding_note<R: Ring>(ctx: &Context<R>, vars: &[String], b: &[usize]) -> String {
    if vars.is_empty() {
        return String::new();
    }
    let parts: Vec<String> = vars
        .iter()
        .zip(b)
        .map(|(v, &c)| format!("{v} = {}", ctx.colours[c].name))
        .collect();
    format!(", {}", parts.join(", "))
}

// ---------------------------------------------------------------------------------------------
// A set of chains to export: `lean/chains/chains.toml`.

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ChainFile {
    chain: Vec<ChainEntry>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ChainEntry {
    /// The theorem's name; the file is named after it in upper camel case.
    name: String,
    context: String,
    lhs: String,
    #[serde(default)]
    rhs: Option<String>,
    #[serde(default)]
    doc: String,
    /// How each side is rewritten: `straighten` (the default: to the basis of crossings and
    /// dots), `shrink` (by rules that make words shorter, until none applies) or `once` (one
    /// step).
    #[serde(default)]
    strategy: Option<String>,
    /// For `shrink` and `once`: the relations whose rules may be used (all if empty).
    #[serde(default)]
    relations: Vec<String>,
}

fn camel(name: &str) -> String {
    name.split('_')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let mut c = p.chars();
            let first = c.next().unwrap().to_ascii_uppercase();
            std::iter::once(first).chain(c).collect::<String>()
        })
        .collect()
}

struct Entry<'a>(&'a ChainEntry);

impl crate::ops::RingVisitor for Entry<'_> {
    type Out = String;
    fn visit<R: Ring>(self, base: R) -> Result<String> {
        let e = self.0;
        let spec = crate::context::ContextSpec::parse(&e.context)?;
        let pres = crate::builtin::presentation(&spec.presentation)?;
        let ctx = Context::new(pres, spec, base)?;
        let strategy = e.strategy.as_deref().unwrap_or("straighten");
        let mut chains = Vec::new();
        if strategy == "straighten" {
            let rules = crate::rewrite::rules(&ctx)?;
            let decl = ctx
                .pres
                .representations
                .iter()
                .find(|d| crate::rep::basis_layout(&ctx, d).is_ok())
                .ok_or_else(|| Error::precondition("no basis of crossings and dots"))?;
            let layout = crate::rep::basis_layout(&ctx, decl)?;
            let ev = crate::rep::evaluator(&ctx, decl)?;
            for side in std::iter::once(&e.lhs).chain(e.rhs.as_ref()) {
                let t = ctx.parse_term(side)?;
                let ch = crate::rewrite::straighten(&ctx, &rules, &layout, &t, 100_000)?;
                if ev.normal_form(&t)? != ch.end {
                    return Err(Error::invalid(format!(
                        "{}: straightening {side} disagrees with the representation",
                        e.name
                    )));
                }
                chains.push(ch);
            }
        } else {
            let once = match strategy {
                "once" => true,
                "shrink" => false,
                s => return Err(Error::invalid(format!("unknown strategy '{s}'"))),
            };
            // The chain is checked against the first representation that decides equality.
            let ev = ctx
                .pres
                .representations
                .iter()
                .find_map(|d| crate::rep::evaluator(&ctx, d).ok())
                .ok_or_else(|| Error::precondition("no representation to check the chain"))?;
            for side in std::iter::once(&e.lhs).chain(e.rhs.as_ref()) {
                let t = ctx.parse_term(side)?;
                let ch = crate::rewrite::shrink(&ctx, &t, &e.relations, once, 10_000)?;
                if ev.normal_form(&t)? != ev.normal_form(&ch.end)? {
                    return Err(Error::invalid(format!(
                        "{}: rewriting {side} disagrees with the representation",
                        e.name
                    )));
                }
                chains.push(ch);
            }
        }
        let mut header = format!(
            "-- Generated by `diagcalc lean-chains` from lean/chains/chains.toml; do not edit.\n\
             -- Context: {}\n-- Left side: {}\n",
            e.context, e.lhs
        );
        if let Some(r) = &e.rhs {
            header.push_str(&format!("-- Right side: {r}\n"));
        }
        header.push('\n');
        let steps: usize = chains.iter().map(|c| c.steps.len()).sum();
        let doc = if e.doc.is_empty() {
            format!("Found by the calculator: a rewrite chain of {steps} steps.")
        } else {
            format!(
                "{} Found by the calculator: a rewrite chain of {steps} steps.",
                e.doc
            )
        };
        export_file(
            &ctx,
            &[Item {
                name: e.name.clone(),
                doc,
                chains: chains.iter().collect(),
            }],
            &header,
        )
    }
}

/// The Lean files for a chain file: one module per chain under `root`, and the root module
/// importing them all, as (path, text).
pub fn export_batch(text: &str, root: &str) -> Result<Vec<(String, String)>> {
    let f: ChainFile =
        toml::from_str(text).map_err(|e| Error::invalid(format!("chain file: {e}")))?;
    let mut out = Vec::new();
    let mut imports = String::new();
    for e in &f.chain {
        let spec = crate::context::ContextSpec::parse(&e.context)?;
        let body = crate::ops::with_ring(&spec.ring, Entry(e))
            .map_err(|err| Error::invalid(format!("{}: {}", e.name, err.message)))?;
        let module = camel(&e.name);
        imports.push_str(&format!("import {root}.{module}\n"));
        out.push((format!("{root}/{module}.lean"), body));
    }
    out.push((
        format!("{root}.lean"),
        format!("-- Generated by `diagcalc lean-chains`; do not edit.\n{imports}"),
    ));
    Ok(out)
}
