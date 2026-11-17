//! The canvas operations in a `klrw` context: `term.layout`, `term.edit`, and `term.diff`
//! (docs/klrw.md, "The canvas").
//!
//! A summand `c b(π; B') x^a e(B)` is drawn with one layer for each dot, bottom up in strand
//! order (canonical layering), and then, when the word has `b(π; B')` or `to(B')`, one layer for
//! the straight-line diagram on top. Strands sit at their positions, not at whole numbers;
//! every boundary lists the positions of its points in `xs`. Ghosts and red strands are drawn as
//! lines of their own kinds.
//!
//! An edit reads the drawn layers bottom to top as a word, inserts or removes layers, and reads
//! the straight-line layer again on the new colours below it: the same permutation and the same
//! top positions. The new word is evaluated, so the result is again in normal form.

use super::klrw::{Elem, Klrw, Summand};
use super::weighted::{Loading, Q};
use crate::error::{Error, Result};
use crate::ops::{RingVisitor, ring_of, with_ring, with_status};
use crate::ops_rewrite::RewriteRequest;
use crate::perm::Perm;
use crate::poly::Poly;
use crate::ring::Ring;
use serde_json::{Value, json};
use std::collections::BTreeMap;

type C<R> = Poly<<R as Ring>::Elem>;

/// The height of the straight-line layer, so that its slopes stay readable.
const STRAIGHT_HEIGHT: f64 = 2.0;

/// One layer of a drawn summand.
#[derive(Clone, Debug, PartialEq)]
enum Layer {
    /// A dot on the strand at this 0-based position.
    Dot(usize),
    /// A crossing at this 0-based position (only while editing).
    Psi(usize),
    /// The straight-line diagram `b(π; top)` or `to(top)`.
    Straight { perm: Perm, top: Loading },
}

fn f64_of(x: &Q) -> f64 {
    let (n, d) = (x.numer().to_string(), x.denom().to_string());
    n.parse::<f64>().unwrap_or(0.0) / d.parse::<f64>().unwrap_or(1.0)
}

/// Rounds for JSON, so that the output does not depend on float printing noise.
fn round(x: f64) -> f64 {
    let r = (x * 1e6).round() / 1e6;
    if r == 0.0 { 0.0 } else { r }
}

/// Runs a canvas or rewriting operation in a `klrw` context.
pub fn run(op: &str, r: &RewriteRequest) -> Value {
    with_status(ring_of(&r.req.context).and_then(|ring| with_ring(&ring, CanvasOp { op, r })))
}

struct CanvasOp<'a> {
    op: &'a str,
    r: &'a RewriteRequest,
}

impl RingVisitor for CanvasOp<'_> {
    type Out = Value;
    fn visit<R: Ring>(self, base: R) -> Result<Value> {
        let spec = crate::context::ContextSpec::parse(&self.r.req.context)?;
        let k = Klrw::new(&spec, base)?;
        let t = k.parse_elem(&self.r.term)?;
        let nf = k.normal_form(&t)?;
        match self.op {
            "term.layout" => {
                let shown = if self.r.shown == 0 {
                    nf.len()
                } else {
                    self.r.shown
                };
                let summands = nf
                    .iter()
                    .enumerate()
                    .take(shown)
                    .map(|(i, s)| {
                        k.layout_summand(s, self.r.orders.get(&(i + 1)).map(Vec::as_slice))
                    })
                    .collect::<Result<Vec<_>>>()?;
                Ok(json!({ "terms": nf.len(), "summands": summands }))
            }
            "term.edit" => k.edit(&t, &nf, self.r),
            "term.diff" => {
                let before = self.r.before.as_deref().ok_or_else(|| {
                    Error::invalid("term.diff needs 'before', the term 'term' replaces")
                })?;
                let old = k.normal_form(&k.parse_elem(before)?)?;
                let limit = if self.r.shown == 0 {
                    usize::MAX
                } else {
                    self.r.shown
                };
                let (prov, truncated) = k.diff_provenance(&old, &nf, limit);
                Ok(json!({
                    "term": k.fmt_summands(&nf),
                    "before": old.iter().map(|s| k.fmt_word(s)).collect::<Vec<_>>(),
                    "provenance": prov,
                    "truncated": truncated,
                }))
            }
            "relations.match" => {
                let s = nf.get(self.r.summand.wrapping_sub(1)).ok_or_else(|| {
                    Error::invalid(format!(
                        "the term has {} summand(s); summands are numbered from 1",
                        nf.len()
                    ))
                })?;
                Ok(json!({ "word": k.fmt_word(s), "matches": [] }))
            }
            op => Err(Error::precondition(format!(
                "{op} is not available in a weighted KLR context: its terms are always in the \
                 basis normal form, and it has no relations to rewrite with (docs/klrw.md)"
            ))),
        }
    }
}

impl<R: Ring> Klrw<R> {
    fn parse_elem(&self, s: &str) -> Result<Elem<C<R>>> {
        self.term(&crate::text::parse(s)?, &BTreeMap::new())
    }

    /// Whether a summand's word has a straight-line part `b(π; B')` or `to(B')`.
    fn has_straight(&self, s: &Summand<C<R>>) -> bool {
        !s.perm.is_identity() || !self.data.isotopic(&s.bottom, &s.top)
    }

    /// The canonical layering: the dots by strand, bottom up, then the straight-line part.
    fn layers(&self, s: &Summand<C<R>>) -> Vec<Layer> {
        let mut out = Vec::new();
        for (k, &a) in s.dots.iter().enumerate() {
            out.extend(std::iter::repeat_n(Layer::Dot(k), a as usize));
        }
        if self.has_straight(s) {
            out.push(Layer::Straight {
                perm: s.perm.clone(),
                top: s.top.clone(),
            });
        }
        out
    }

    fn layer_generator(l: &Layer) -> &'static str {
        match l {
            Layer::Dot(_) => "x",
            Layer::Psi(_) => "psi",
            Layer::Straight { perm, .. } if perm.is_identity() => "to",
            Layer::Straight { .. } => "b",
        }
    }

    /// The drawn order as canonical indices (0-based), checked: the straight-line layer meets
    /// every strand, so nothing passes it.
    fn drawn(&self, canon: &[Layer], order: Option<&[usize]>) -> Result<Vec<usize>> {
        let n = canon.len();
        let Some(order) = order else {
            return Ok((0..n).collect());
        };
        if order.len() != n {
            return Err(Error::invalid(format!(
                "the order names {} layers; the summand has {n}",
                order.len()
            )));
        }
        let mut seen = vec![false; n];
        let mut rank = Vec::with_capacity(n);
        for &l in order {
            let c = l
                .checked_sub(1)
                .filter(|&c| c < n && !seen[c])
                .ok_or_else(|| Error::invalid("the order must list each layer once, from 1"))?;
            seen[c] = true;
            rank.push(c);
        }
        if let Some(i) = rank
            .iter()
            .position(|&c| matches!(canon[c], Layer::Straight { .. }))
            && i + 1 < n
        {
            return Err(Error::precondition(format!(
                "layers {} and {} share strands: one cannot pass the other",
                rank[i] + 1,
                rank[i + 1] + 1
            )));
        }
        Ok(rank)
    }

    /// One summand drawn for the canvas, in the shape of `term.layout`'s `layout`.
    fn layout_summand(&self, s: &Summand<C<R>>, order: Option<&[usize]>) -> Result<Value> {
        let canon = self.layers(s);
        let rank = self.drawn(&canon, order)?;
        let n = s.bottom.n();
        let colour = |c: u16| self.klr.colours[c as usize].name.clone();
        // the horizontal extent: points, ghosts, and red strands at both ends
        let mut ext: Vec<f64> = Vec::new();
        for l in [&s.bottom, &s.top] {
            for (c, x) in &l.0 {
                ext.push(f64_of(x));
                for e in &self.data.edges {
                    if e.head == *c {
                        ext.push(f64_of(&(x + &e.weight)));
                    }
                }
            }
        }
        for r in &self.data.reds {
            ext.push(f64_of(&r.pos));
        }
        let lo = ext.iter().cloned().fold(f64::INFINITY, f64::min);
        let lo = if lo.is_finite() { lo } else { 0.0 };
        let hi = ext.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let width = if hi.is_finite() { hi - lo } else { 0.0 };
        let sx = |x: &Q| round(f64_of(x) - lo);
        let xb: Vec<f64> = s.bottom.0.iter().map(|p| sx(&p.1)).collect();
        let xt: Vec<f64> = s.top.0.iter().map(|p| sx(&p.1)).collect();
        let ghosts = |c: u16| -> Vec<f64> {
            self.data
                .edges
                .iter()
                .filter(|e| e.head == c)
                .map(|e| f64_of(&e.weight))
                .collect()
        };
        let dots = canon.iter().filter(|l| matches!(l, Layer::Dot(_))).count();
        let straight = canon.len() > dots;
        let height = if straight {
            dots as f64 + STRAIGHT_HEIGHT
        } else {
            (dots as f64).max(1.0)
        };
        let mut lines: Vec<Value> = Vec::new();
        for r in &self.data.reds {
            let x = sx(&r.pos);
            lines.push(json!({ "kind": "red", "x0": x, "y0": 0.0, "x1": x, "y1": height }));
        }
        for (j, (c, _)) in s.bottom.0.iter().enumerate() {
            lines.push(
                json!({ "kind": "label", "x": xb[j], "y": 0.0, "text": colour(*c), "strand": j }),
            );
        }
        // a row of straight segments from `a` to `b`, the one on `hot` tagged with `layer`
        let row = |lines: &mut Vec<Value>,
                   y0: f64,
                   y1: f64,
                   a: &[f64],
                   b: &[f64],
                   hot: Option<(usize, usize)>| {
            for (j, (c, _)) in s.bottom.0.iter().enumerate() {
                for w in ghosts(*c) {
                    lines.push(json!({
                        "kind": "ghost", "strand": j,
                        "x0": round(a[j] + w), "y0": y0, "x1": round(b[j] + w), "y1": y1,
                    }));
                }
                let mut v = json!({ "kind": "strand", "strand": j, "x0": a[j], "y0": y0, "x1": b[j], "y1": y1 });
                if let Some((layer, on)) = hot
                    && (on == usize::MAX || on == j)
                {
                    v["layer"] = json!(layer);
                }
                lines.push(v);
            }
        };
        let mut pieces = Vec::new();
        // the top position of each strand
        let ends: Vec<f64> = (0..n).map(|j| xt[s.perm.0[j] as usize]).collect();
        if canon.is_empty() {
            row(&mut lines, 0.0, 1.0, &xb, &ends, None);
        }
        let colours = s.bottom.colours();
        for (i, &c) in rank.iter().enumerate() {
            let y0 = i as f64;
            match &canon[c] {
                Layer::Dot(k) => {
                    row(&mut lines, y0, y0 + 1.0, &xb, &xb, Some((c + 1, *k)));
                    let x = xb[*k];
                    lines.push(json!({ "kind": "dot", "x": x, "y": y0 + 0.5, "layer": c + 1, "strand": k }));
                    pieces.push(json!({
                        "layer": c + 1, "generator": "x", "draw": "dot", "odd": false,
                        "degree": 2 * self.data.d[colours[*k] as usize],
                        "box": [round(x - 0.25), y0 + 0.25, round(x + 0.25), y0 + 0.75],
                        "bottom": k, "top": k, "pos": k, "width": 1,
                        "inputs": [k], "outputs": [k],
                    }));
                }
                l @ Layer::Straight { .. } => {
                    row(
                        &mut lines,
                        y0,
                        y0 + STRAIGHT_HEIGHT,
                        &xb,
                        &ends,
                        Some((c + 1, usize::MAX)),
                    );
                    let mut bare = s.clone();
                    bare.dots = vec![0; n];
                    let inv = s.perm.inverse();
                    let all = xb.iter().chain(&xt).cloned();
                    let x0 = all.clone().fold(f64::INFINITY, f64::min);
                    let x1 = all.fold(f64::NEG_INFINITY, f64::max);
                    pieces.push(json!({
                        "layer": c + 1, "generator": Self::layer_generator(l), "draw": "straight",
                        "odd": false, "degree": self.degree(&bare)?,
                        "box": [x0, y0, x1, y0 + STRAIGHT_HEIGHT],
                        "bottom": 0, "top": 0, "pos": 0, "width": n,
                        "inputs": (0..n).collect::<Vec<_>>(),
                        "outputs": inv.0.iter().map(|&j| j as usize).collect::<Vec<_>>(),
                    }));
                }
                Layer::Psi(_) => unreachable!("only edits insert crossings"),
            }
        }
        let names = |l: &Loading| l.0.iter().map(|p| colour(p.0)).collect::<Vec<_>>();
        let mut boundaries =
            vec![json!({ "y": 0.0, "below": 0, "object": names(&s.bottom), "xs": xb })];
        for i in 1..=rank.len() {
            let top = straight && i == rank.len();
            boundaries.push(if top {
                json!({ "y": height, "below": i, "object": names(&s.top), "xs": xt })
            } else {
                json!({ "y": i as f64, "below": i, "object": names(&s.bottom), "xs": xb })
            });
        }
        let (neg, coefficient) = self.fmt_coeff(&s.coeff);
        Ok(json!({
            "text": self.fmt_summands(std::slice::from_ref(s)),
            "word": self.fmt_word(s),
            "degree": self.degree(s)?,
            "source": self.fmt_loading(&s.bottom),
            "target": self.fmt_loading(&s.top),
            "negative": neg,
            "coefficient": coefficient,
            "sign": 1,
            "order": rank.iter().map(|r| r + 1).collect::<Vec<_>>(),
            "width": round(width),
            "height": height,
            "pieces": pieces,
            "strands": colours.iter().map(|&c| json!({ "colour": colour(c), "up": true })).collect::<Vec<_>>(),
            "lines": lines,
            "boundaries": boundaries,
        }))
    }

    /// The word of drawn layers on `bottom`, bottom up, with every straight-line layer read
    /// again on the colours below it.
    fn word_text(&self, bottom: &Loading, drawn: &[Layer]) -> Result<String> {
        let n = bottom.n();
        let mut cur = bottom.clone();
        let mut parts = vec![format!("e({})", self.fmt_loading(bottom))];
        for l in drawn {
            match l {
                Layer::Dot(k) => parts.push(format!("x_{}", k + 1)),
                Layer::Psi(k) => {
                    parts.push(format!("psi_{}", k + 1));
                    let (a, b) = (cur.0[*k].0, cur.0[k + 1].0);
                    cur.0[*k].0 = b;
                    cur.0[k + 1].0 = a;
                }
                Layer::Straight { perm, top } => {
                    let mut to = top.clone();
                    for j in 0..n {
                        to.0[perm.0[j] as usize].0 = cur.0[j].0;
                    }
                    parts.push(if perm.is_identity() {
                        format!("to({})", self.fmt_loading(&to))
                    } else {
                        let ent: Vec<String> =
                            perm.0.iter().map(|&k| (k + 1).to_string()).collect();
                        format!("b({}; {})", ent.join(" "), self.fmt_loading(&to))
                    });
                    cur = to;
                }
            }
        }
        parts.reverse();
        Ok(parts.join(" "))
    }

    /// `term.edit`: insert a dot or a crossing, or remove layers, in one summand.
    fn edit(&self, t: &Elem<C<R>>, nf: &[Summand<C<R>>], r: &RewriteRequest) -> Result<Value> {
        let k = r.summand;
        let s = nf.get(k.wrapping_sub(1)).ok_or_else(|| {
            Error::invalid(format!(
                "the term has {} summand(s); summands are numbered from 1",
                nf.len()
            ))
        })?;
        let canon = self.layers(s);
        let rank = self.drawn(&canon, r.orders.get(&k).map(Vec::as_slice))?;
        let mut drawn: Vec<Layer> = rank.iter().map(|&c| canon[c].clone()).collect();
        let n = s.bottom.n();
        match r.action.as_str() {
            "insert" => {
                let h = r.below;
                if h > drawn.len() {
                    return Err(Error::invalid(format!(
                        "the summand has {} layers: none can go above {h}",
                        drawn.len()
                    )));
                }
                let pos = r
                    .position
                    .checked_sub(1)
                    .ok_or_else(|| Error::invalid("positions are numbered from 1"))?;
                let l = match r.generator.as_str() {
                    "x" if pos < n => Layer::Dot(pos),
                    "psi" if pos + 1 < n => Layer::Psi(pos),
                    "x" | "psi" => {
                        return Err(Error::precondition(format!(
                            "{} does not fit at position {} of {n} strands",
                            r.generator,
                            pos + 1
                        )));
                    }
                    g => {
                        return Err(Error::invalid(format!(
                            "there is no generator '{g}': a weighted KLR canvas inserts x and psi"
                        )));
                    }
                };
                drawn.insert(h, l);
            }
            "remove" => {
                let asked = if r.layers.is_empty() {
                    vec![r.layer]
                } else {
                    r.layers.clone()
                };
                let mut at = Vec::new();
                for l in asked {
                    let c = l
                        .checked_sub(1)
                        .filter(|&c| c < canon.len())
                        .ok_or_else(|| Error::invalid(format!("there is no layer {l}")))?;
                    at.push(rank.iter().position(|&x| x == c).expect("permutation"));
                }
                at.sort_unstable();
                at.dedup();
                for &i in at.iter().rev() {
                    drawn.remove(i);
                }
            }
            a => {
                return Err(Error::invalid(format!(
                    "action '{a}': use insert or remove"
                )));
            }
        }
        let text = self.word_text(&s.bottom, &drawn)?;
        let new = self.scale(&s.coeff, &self.parse_elem(&text)?);
        let old = self.summand_elem(s)?;
        let out = self.add(&self.add(t, &self.neg(&old)), &new);
        let out_nf = self.normal_form(&out)?;
        let new_nf = self.normal_form(&new)?;
        let new_words: Vec<String> = new_nf.iter().map(|x| self.fmt_word(x)).collect();
        // provenance: summands already there keep their layers; the edited summand's new
        // diagrams keep what lines up with the old one
        let prov: Vec<Value> = out_nf
            .iter()
            .map(|x| {
                let w = self.fmt_word(x);
                let layers = self.layers(x);
                let from: Vec<Vec<[usize; 2]>> =
                    if let Some(j) = nf.iter().position(|y| self.fmt_word(y) == w) {
                        (0..layers.len()).map(|l| vec![[j + 1, l + 1]]).collect()
                    } else if new_words.contains(&w) {
                        self.align(s, x)
                            .iter()
                            .map(|o| o.map(|l| vec![[k, l + 1]]).unwrap_or_default())
                            .collect()
                    } else {
                        vec![Vec::new(); layers.len()]
                    };
                self.provenance_entry(x, from)
            })
            .collect();
        let word = if new_nf.len() == 1 {
            self.fmt_word(&new_nf[0])
        } else {
            self.fmt_summands(&new_nf)
        };
        Ok(json!({
            "term": self.fmt_summands(&out_nf),
            "word": word,
            "provenance": prov,
        }))
    }

    fn provenance_entry(&self, x: &Summand<C<R>>, from: Vec<Vec<[usize; 2]>>) -> Value {
        let layers: Vec<Value> = self
            .layers(x)
            .iter()
            .zip(from)
            .map(|(l, f)| json!({ "generator": Self::layer_generator(l), "from": f }))
            .collect();
        json!({ "word": self.fmt_word(x), "layers": layers })
    }

    /// For each canonical layer of `new`, the canonical layer of `old` it lines up with: the
    /// dots on the same strand, as many as both have, and the straight-line layer when the
    /// permutation is the same.
    fn align(&self, old: &Summand<C<R>>, new: &Summand<C<R>>) -> Vec<Option<usize>> {
        let (lo, ln) = (self.layers(old), self.layers(new));
        let mut out = Vec::with_capacity(ln.len());
        let mut start = Vec::new(); // first canonical dot layer of each strand of `old`
        let mut acc = 0;
        for &a in &old.dots {
            start.push(acc);
            acc += a as usize;
        }
        let mut seen = vec![0usize; new.dots.len()];
        for l in &ln {
            out.push(match l {
                Layer::Dot(k) => {
                    let u = seen[*k];
                    seen[*k] += 1;
                    (old.dots.len() == new.dots.len() && u < old.dots[*k] as usize)
                        .then(|| start[*k] + u)
                }
                Layer::Straight { perm, .. } => lo
                    .iter()
                    .position(|m| matches!(m, Layer::Straight { perm: p, .. } if p == perm)),
                Layer::Psi(_) => None,
            });
        }
        out
    }

    /// `term.diff`: the summands of `new` that are in `old` keep their layers; the others are
    /// paired with the changed summands of `old` they line up with best.
    fn diff_provenance(
        &self,
        old: &[Summand<C<R>>],
        new: &[Summand<C<R>>],
        limit: usize,
    ) -> (Value, bool) {
        let ow: Vec<String> = old.iter().map(|s| self.fmt_word(s)).collect();
        let nw: Vec<String> = new.iter().map(|s| self.fmt_word(s)).collect();
        let gone: Vec<usize> = (0..old.len()).filter(|&j| !nw.contains(&ow[j])).collect();
        let came: Vec<usize> = (0..new.len()).filter(|&j| !ow.contains(&nw[j])).collect();
        let truncated = gone.len() > limit || came.len() > limit;
        let (gone, came) = (
            &gone[..gone.len().min(limit)],
            &came[..came.len().min(limit)],
        );
        let mut pairs = Vec::new();
        for (a, &j) in came.iter().enumerate() {
            for (b, &i) in gone.iter().enumerate() {
                let al = self.align(&old[i], &new[j]);
                let kept = al.iter().filter(|o| o.is_some()).count();
                if kept > 0 {
                    pairs.push((kept, a, b, al));
                }
            }
        }
        pairs.sort_by(|x, y| y.0.cmp(&x.0).then(x.1.cmp(&y.1)).then(x.2.cmp(&y.2)));
        let mut from: BTreeMap<usize, Vec<Vec<[usize; 2]>>> = BTreeMap::new();
        let (mut a_done, mut b_done) = (vec![false; came.len()], vec![false; gone.len()]);
        for (_, a, b, al) in pairs {
            if a_done[a] || b_done[b] {
                continue;
            }
            a_done[a] = true;
            b_done[b] = true;
            let i = gone[b];
            from.insert(
                came[a],
                al.iter()
                    .map(|o| o.map(|l| vec![[i + 1, l + 1]]).unwrap_or_default())
                    .collect(),
            );
        }
        let prov: Vec<Value> = new
            .iter()
            .enumerate()
            .map(|(j, x)| {
                let nl = self.layers(x).len();
                let f = if let Some(i) = ow.iter().position(|w| *w == nw[j]) {
                    (0..nl).map(|l| vec![[i + 1, l + 1]]).collect()
                } else {
                    from.remove(&j).unwrap_or_else(|| vec![Vec::new(); nl])
                };
                self.provenance_entry(x, f)
            })
            .collect();
        (json!(prov), truncated)
    }
}

#[cfg(test)]
mod tests {
    use crate::ops;
    use serde_json::{Value, json};

    const CTX: &str = "klrw type=A2 theta_ji=1/2";

    fn call(op: &str, req: Value) -> Value {
        let v = ops::call(op, &req);
        assert_eq!(v["status"], "ok", "{op}: {v:#}");
        v
    }

    #[test]
    fn layout_puts_strands_at_their_positions() {
        let v = call(
            "term.layout",
            json!({ "context": CTX, "term": "x_1 b(2 1; j@0 i@1) e(i@0 j@1)" }),
        );
        let s = &v["summands"][0];
        assert_eq!(s["order"], json!([1, 2]));
        assert_eq!(s["pieces"][0]["generator"], "x");
        assert_eq!(s["pieces"][1]["generator"], "b");
        assert_eq!(s["boundaries"][0]["xs"].as_array().unwrap().len(), 2);
        let kinds: Vec<&str> = s["lines"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l["kind"].as_str().unwrap())
            .collect();
        assert!(kinds.contains(&"ghost"), "{s:#}");
    }

    #[test]
    fn edits_insert_and_remove() {
        let v = call(
            "term.edit",
            json!({ "context": CTX, "term": "e(i@0 j@1)", "action": "insert",
                    "generator": "x", "below": 0, "position": 2 }),
        );
        assert_eq!(v["term"], "x_2 e(i@0 j@1)");
        let v = call(
            "term.edit",
            json!({ "context": CTX, "term": "x_2 e(i@0 j@1)", "action": "remove", "layer": 1 }),
        );
        assert_eq!(v["term"], "e(i@0 j@1)");
        let v = call(
            "term.edit",
            json!({ "context": CTX, "term": "e(i@0 j@1)", "action": "insert",
                    "generator": "psi", "below": 0, "position": 1 }),
        );
        let t = v["term"].as_str().unwrap();
        assert!(t.starts_with("b(2 1;"), "{t}");
        assert_eq!(v["provenance"][0]["layers"][0]["generator"], "b");
        assert_eq!(v["provenance"][0]["layers"][0]["from"], json!([]));
    }

    #[test]
    fn a_dot_cannot_pass_the_straight_layer() {
        let v = ops::call(
            "term.layout",
            &json!({ "context": CTX, "term": "x_1 b(2 1; j@0 i@1) e(i@0 j@1)",
                     "orders": { "1": [2, 1] } }),
        );
        assert_eq!(v["status"], "error");
    }

    #[test]
    fn diff_keeps_unchanged_summands() {
        let v = call(
            "term.diff",
            json!({ "context": CTX, "term": "x_1^2 e(i@0 j@1) + e(j@0 i@1)",
                    "before": "x_1 e(i@0 j@1) + e(j@0 i@1)" }),
        );
        let p = &v["provenance"];
        let first = &p[0]["layers"];
        assert_eq!(first[0]["from"], json!([[1, 1]]));
        assert_eq!(first[1]["from"], json!([]));
    }

    #[test]
    fn rewriting_says_why_not() {
        let v = ops::call(
            "slide",
            &json!({ "context": CTX, "term": "x_1 e(i@0 j@1)", "layer": 1, "direction": "up" }),
        );
        assert_eq!(v["status"], "error");
        let v = call(
            "relations.match",
            json!({ "context": CTX, "term": "x_1 e(i@0 j@1)" }),
        );
        assert_eq!(v["matches"], json!([]));
    }
}
