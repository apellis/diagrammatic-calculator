//! The rewriting operations: `relations.match`, `relation.apply`, `interchange`, `slide`,
//! `straighten`, `chain.find` and `export.lean`. Schemas in `spec/ops/`.
//!
//! Locations: a term's summands are numbered from 1 in printed order; a summand's layers are
//! numbered from 1 at the bottom of its canonical layering, strands from 1 at the left.

use crate::context::{Context, ContextSpec};
use crate::elab::CTerm;
use crate::error::{Error, Result};
use crate::lean;
use crate::ops::{Request, RingVisitor, load_presentation, ring_of, with_ring, with_status};
use crate::rep::{self, Evaluator};
use crate::rewrite::{self, Chain, Layout, Match, Rect, Rule, Step};
use crate::ring::Ring;
use crate::term::Word;
use serde_json::{Value, json};

/// What a rewriting request asks for.
#[derive(Clone, Debug, Default)]
pub struct RewriteRequest {
    pub req: Request,
    pub term: String,
    /// The other side of an equation (`chain.find`, `export.lean`).
    pub rhs: Option<String>,
    /// 1-based summand of `term`.
    pub summand: usize,
    /// 1-based inclusive layers and strands.
    pub rectangle: Option<((usize, usize), (usize, usize))>,
    /// Only rules of this relation.
    pub relation: Option<String>,
    /// 0-based index into the matches (`relation.apply`).
    pub index: usize,
    /// `relation.apply` on every summand.
    pub everywhere: bool,
    /// 1-based layer (`interchange`: the lower of two; `slide`: the piece).
    pub layer: usize,
    /// `slide`: `up` or `down`.
    pub direction: String,
    pub max_steps: usize,
    /// `export.lean`: the theorem's name and docstring.
    pub name: String,
    pub doc: String,
}

/// Runs a rewriting operation.
pub fn run(op: &str, r: &RewriteRequest) -> Value {
    with_status(ring_of(&r.req.context).and_then(|ring| with_ring(&ring, RewriteOp { op, r })))
}

pub struct RewriteOp<'a> {
    pub op: &'a str,
    pub r: &'a RewriteRequest,
}

impl RingVisitor for RewriteOp<'_> {
    type Out = Value;
    fn visit<R: Ring>(self, base: R) -> Result<Value> {
        let spec = ContextSpec::parse(&self.r.req.context)?;
        let pres = load_presentation(&spec, self.r.req.presentation.as_deref())?;
        let ctx = Context::new(pres, spec, base)?;
        let t = ctx.parse_term(&self.r.term)?;
        let rules: Vec<Rule<R>> = rewrite::rules(&ctx)?
            .into_iter()
            .filter(|r| self.r.relation.as_ref().is_none_or(|n| &r.relation == n))
            .collect();
        if rules.is_empty()
            && let Some(n) = &self.r.relation
        {
            return Err(Error::invalid(format!("no relation named '{n}'")));
        }
        match self.op {
            "relations.match" => {
                let (w, ms) = matches(&ctx, &rules, &t, self.r)?;
                let list: Vec<Value> = ms
                    .iter()
                    .enumerate()
                    .map(|(i, m)| {
                        let rule = &rules[m.rule];
                        let mut v = match_json(&ctx, rule, m);
                        v["index"] = json!(i);
                        let mut chain = Chain::new(ctx.monomial_term(&w, ctx.coeff.one()));
                        rewrite::rewrite(&ctx, &rules, &mut chain, &w, m.clone())
                            .expect("word in term");
                        v["preview"] = json!(ctx.fmt_term(&chain.end));
                        v
                    })
                    .collect();
                Ok(json!({ "word": ctx.fmt_word(&w), "matches": list }))
            }
            "relation.apply" => {
                let chain = if self.r.everywhere {
                    rewrite::apply_everywhere(&ctx, &rules, &t)?
                } else {
                    let (w, ms) = matches(&ctx, &rules, &t, self.r)?;
                    let m = ms.into_iter().nth(self.r.index).ok_or_else(|| {
                        Error::invalid(format!("there is no match {}", self.r.index))
                    })?;
                    let mut chain = Chain::new(t.clone());
                    rewrite::rewrite(&ctx, &rules, &mut chain, &w, m)?;
                    chain
                };
                Ok(chain_json(&ctx, &chain))
            }
            "interchange" => {
                let w = summand(&ctx, &t, self.r.summand)?;
                let i = self
                    .r
                    .layer
                    .checked_sub(1)
                    .ok_or_else(|| Error::invalid("layers are numbered from 1"))?;
                if i + 1 >= w.layers.len() {
                    return Err(Error::invalid(format!(
                        "the summand has {} layers: there is no layer above {}",
                        w.layers.len(),
                        i + 1
                    )));
                }
                let (lo, up, odd) = ctx
                    .swap_adjacent(&w.layers[i], &w.layers[i + 1])
                    .ok_or_else(|| {
                        Error::precondition(format!(
                            "layers {} and {} share strands: they do not interchange",
                            i + 1,
                            i + 2
                        ))
                    })?;
                let mut moved = w.clone();
                moved.layers[i] = lo;
                moved.layers[i + 1] = up;
                let (c, sign) = ctx.canonical(&moved);
                // The term is unchanged: interchange is an equality of diagrams.
                Ok(json!({
                    "sign": if odd { -1 } else { 1 },
                    "same_diagram": c == w && sign == odd,
                    "term": ctx.fmt_term(&t),
                }))
            }
            "slide" => {
                let layout = layout(&ctx, self.r.req.strategy.as_deref())?;
                let w = summand(&ctx, &t, self.r.summand)?;
                let up = match self.r.direction.as_str() {
                    "up" => true,
                    "down" => false,
                    d => return Err(Error::invalid(format!("direction '{d}': use up or down"))),
                };
                let chain = slide(&ctx, &rules, &layout, &t, &w, self.r.layer, up)?;
                Ok(chain_json(&ctx, &chain))
            }
            "straighten" => {
                let (chain, ev, layout_name) = straighten(&ctx, &rules, &t, self.r)?;
                let want = ev.normal_form(&t)?;
                let mut v = chain_json(&ctx, &chain);
                v["agrees"] = json!(want == chain.end);
                v["basis"] = json!(layout_name);
                Ok(v)
            }
            "chain.find" | "export.lean" => {
                let (lhs_chain, ev, _) = straighten(&ctx, &rules, &t, self.r)?;
                let rhs_chain = match &self.r.rhs {
                    Some(s) => {
                        let u = ctx.parse_term(s)?;
                        Some(straighten(&ctx, &rules, &u, self.r)?.0)
                    }
                    None => None,
                };
                let equal = rhs_chain.as_ref().is_none_or(|c| c.end == lhs_chain.end);
                if self.op == "chain.find" {
                    let mut chains = vec![chain_json(&ctx, &lhs_chain)];
                    if let Some(c) = &rhs_chain {
                        chains.push(chain_json(&ctx, c));
                    }
                    let agrees = ev.normal_form(&t)? == lhs_chain.end;
                    return Ok(json!({ "equal": equal, "agrees": agrees, "chains": chains }));
                }
                if !equal {
                    return Err(Error::precondition(
                        "the two sides straighten to different normal forms: they are not equal",
                    ));
                }
                let mut chains = vec![&lhs_chain];
                if let Some(c) = &rhs_chain {
                    chains.push(c);
                }
                let steps: usize = chains.iter().map(|c| c.steps.len()).sum();
                let name = if self.r.name.is_empty() {
                    "diagcalc_chain".to_string()
                } else {
                    self.r.name.clone()
                };
                let text = lean::export_file(
                    &ctx,
                    &[lean::Item {
                        name: name.clone(),
                        doc: self.r.doc.clone(),
                        chains,
                    }],
                    "",
                )?;
                Ok(json!({ "theorem": name, "steps": steps, "lean": text }))
            }
            op => Err(Error::invalid(format!(
                "unknown rewriting operation '{op}'"
            ))),
        }
    }
}

fn summand<R: Ring>(ctx: &Context<R>, t: &CTerm<R>, k: usize) -> Result<Word> {
    let _ = ctx;
    t.terms
        .get(k.wrapping_sub(1))
        .map(|(w, _)| w.clone())
        .ok_or_else(|| {
            Error::invalid(format!(
                "the term has {} summand(s); summands are numbered from 1",
                t.terms.len()
            ))
        })
}

fn matches<R: Ring>(
    ctx: &Context<R>,
    rules: &[Rule<R>],
    t: &CTerm<R>,
    r: &RewriteRequest,
) -> Result<(Word, Vec<Match>)> {
    let w = summand(ctx, t, r.summand)?;
    let rect = r.rectangle.map(|((l0, l1), (s0, s1))| Rect {
        layers: (l0.saturating_sub(1), l1),
        strands: (s0.saturating_sub(1), s1),
    });
    let ms = rewrite::find_matches(ctx, rules, &w, rect.as_ref());
    Ok((w, ms))
}

fn binding_json<R: Ring>(ctx: &Context<R>, rule: &Rule<R>) -> Value {
    let vars = &ctx
        .pres
        .relations
        .iter()
        .find(|r| r.name == rule.relation)
        .expect("relation")
        .vars;
    let mut m = serde_json::Map::new();
    for (v, &c) in vars.iter().zip(&rule.binding) {
        m.insert(v.clone(), json!(ctx.colours[c].name));
    }
    Value::Object(m)
}

fn match_json<R: Ring>(ctx: &Context<R>, rule: &Rule<R>, m: &Match) -> Value {
    json!({
        "relation": rule.relation,
        "direction": if rule.forward { "left to right" } else { "right to left" },
        "binding": binding_json(ctx, rule),
        "pattern": ctx.fmt_word(&rule.pattern),
        "replacement": ctx.fmt_term(&rule.replacement),
        "occurrence": m.occurrence.iter().map(|i| i + 1).collect::<Vec<_>>(),
        "moves": m.swaps.iter().map(|i| i + 1).collect::<Vec<_>>(),
        "sign": if m.sign { -1 } else { 1 },
        "rectangle": {
            "layers": [m.at + 1, m.at + rule.pattern.layers.len()],
            "strands": [m.window + 1, m.window + rule.pattern.source.len()],
        },
    })
}

fn step_json<R: Ring>(ctx: &Context<R>, st: &Step<R>) -> Value {
    let mut v = match_json(ctx, &st.rule, &st.at);
    v["word"] = json!(ctx.fmt_term(&ctx.normalize(vec![(st.word.clone(), st.coeff.clone())])));
    v["results"] = st
        .summands
        .iter()
        .map(|s| {
            let mut c = ctx.coeff.mul(&st.coeff, &s.coeff);
            if st.at.sign ^ s.sign {
                c = ctx.coeff.neg(&c);
            }
            json!({
                "term": ctx.fmt_term(&ctx.normalize(vec![(s.word.clone(), c)])),
                "from": s.origin.iter().map(|o| o.map(|i| i + 1)).collect::<Vec<_>>(),
            })
        })
        .collect();
    v
}

fn chain_json<R: Ring>(ctx: &Context<R>, ch: &Chain<R>) -> Value {
    json!({
        "start": ctx.fmt_term(&ch.start),
        "term": ctx.fmt_term(&ch.end),
        "steps": ch.steps.iter().map(|s| step_json(ctx, s)).collect::<Vec<_>>(),
    })
}

fn layout<R: Ring>(ctx: &Context<R>, strategy: Option<&str>) -> Result<Layout> {
    let decl = basis_decl(ctx, strategy)?;
    rep::basis_layout(ctx, decl)
}

fn basis_decl<'c, R: Ring>(
    ctx: &'c Context<R>,
    strategy: Option<&str>,
) -> Result<&'c crate::pres::RepresentationDecl> {
    let mut decls = ctx.pres.representations.iter();
    match strategy {
        Some(n) => decls
            .find(|d| d.name == n)
            .ok_or_else(|| Error::invalid(format!("no representation named '{n}'"))),
        None => decls
            .find(|d| rep::basis_layout(ctx, d).is_ok())
            .ok_or_else(|| {
                Error::precondition(format!(
                    "presentation {} declares no basis of crossings and dots to straighten to",
                    ctx.pres.name
                ))
            }),
    }
}

/// A straightening chain, the representation whose basis it reaches, and that basis's name.
type Straightened<'c, R> = (Chain<R>, Box<dyn Evaluator<R> + 'c>, String);

fn straighten<'c, R: Ring>(
    ctx: &'c Context<R>,
    rules: &[Rule<R>],
    t: &CTerm<R>,
    r: &RewriteRequest,
) -> Result<Straightened<'c, R>> {
    let decl = basis_decl(ctx, r.req.strategy.as_deref())?;
    let layout = rep::basis_layout(ctx, decl)?;
    let ev = rep::evaluator(ctx, decl)?;
    let cap = if r.max_steps == 0 {
        100_000
    } else {
        r.max_steps
    };
    let chain = rewrite::straighten(ctx, rules, &layout, t, cap)?;
    Ok((chain, ev, decl.name.clone()))
}

/// Slides the dot at `layer` of `w` through the crossings above (or below) it, one dot-slide
/// relation at a time; the correction terms stay where they appear.
fn slide<R: Ring>(
    ctx: &Context<R>,
    rules: &[Rule<R>],
    layout: &Layout,
    t: &CTerm<R>,
    w: &Word,
    layer: usize,
    up: bool,
) -> Result<Chain<R>> {
    let mut dot = layer
        .checked_sub(1)
        .filter(|&i| i < w.layers.len())
        .ok_or_else(|| Error::invalid(format!("there is no layer {layer}")))?;
    if w.layers[dot].generator != layout.dot {
        return Err(Error::invalid(format!("layer {layer} is not a dot")));
    }
    let mut chain = Chain::new(t.clone());
    let mut cur = w.clone();
    for _ in 0..cur.layers.len() + 1 {
        let next = rewrite::neighbour(ctx, &cur, dot, up)
            .filter(|&c| cur.layers[c].generator == layout.crossing);
        let Some(c) = next else {
            return Ok(chain);
        };
        let set = if up { vec![dot, c] } else { vec![c, dot] };
        let m = rewrite::match_on(ctx, rules, &cur, &set).ok_or_else(|| {
            Error::precondition(format!(
                "no relation moves the dot at layer {} through the crossing at layer {}",
                dot + 1,
                c + 1
            ))
        })?;
        rewrite::rewrite(ctx, rules, &mut chain, &cur, m)?;
        let st = chain.steps.last().unwrap();
        // Follow the dot into the result that keeps the crossing.
        let keep = st.summands.iter().find(|s| {
            s.word.layers.len() == cur.layers.len()
                && s.word
                    .layers
                    .iter()
                    .zip(&s.origin)
                    .any(|(l, o)| o.is_none() && l.generator == layout.crossing)
        });
        let Some(s) = keep else {
            return Ok(chain);
        };
        let new_dot = (0..s.word.layers.len())
            .find(|&j| s.origin[j].is_none() && s.word.layers[j].generator == layout.dot)
            .ok_or_else(|| Error::invalid("internal: the slid dot is lost"))?;
        cur = s.word.clone();
        dot = new_dot;
    }
    Ok(chain)
}

#[cfg(test)]
mod tests {
    use crate::ops::call;
    use serde_json::json;

    #[test]
    fn rewriting_operations() {
        let v = call(
            "relations.match",
            &json!({ "context": "klr type=A2 nu=i+j", "term": "psi_1 x_2 psi_1 e(ij)" }),
        );
        assert_eq!(v["status"], "ok", "{v}");
        let ms = v["matches"].as_array().unwrap();
        assert!(ms.len() >= 2, "{v}");
        let v = call(
            "relation.apply",
            &json!({ "context": "klr type=A2 nu=i+j", "term": "psi_1 x_2 psi_1 e(ij)",
                     "relation": "dot-slide-right-distinct" }),
        );
        assert_eq!(v["status"], "ok", "{v}");
        assert_eq!(v["term"], "psi_1 psi_1 x_1 e(ij)", "{v}");
        let v = call(
            "straighten",
            &json!({ "context": "klr type=A2 nu=i+j", "term": "psi_1 x_2 psi_1 e(ij)" }),
        );
        assert_eq!(v["agrees"], true, "{v}");
        assert_eq!(v["term"], "x_1^2 e(ij) + x_1 x_2 e(ij)", "{v}");
        let v = call(
            "slide",
            &json!({ "context": "nilhecke nu=3", "term": "x_1 psi_1 psi_2 psi_1",
                     "layer": 4, "direction": "down" }),
        );
        assert_eq!(v["status"], "ok", "{v}");
        assert_eq!(v["steps"].as_array().unwrap().len(), 2, "{v}");
        let v = call(
            "interchange",
            &json!({ "context": "nilhecke nu=3", "term": "x_1 x_3", "layer": 1 }),
        );
        assert_eq!(v["same_diagram"], true, "{v}");
        let v = call(
            "chain.find",
            &json!({ "context": "klr type=A2 nu=2i+j", "term": "psi_1 psi_2 psi_1 e(iji)",
                     "rhs": "psi_2 psi_1 psi_2 e(iji) + e(iji)" }),
        );
        assert_eq!(v["equal"], true, "{v}");
        let v = call(
            "export.lean",
            &json!({ "context": "klr type=A2 nu=i+j", "term": "psi_1 x_2 psi_1 e(ij)",
                     "rhs": "x_1^2 e(ij) + x_1 x_2 e(ij)", "name": "t" }),
        );
        assert!(v["lean"].as_str().unwrap().contains("theorem t "), "{v}");
    }
}
