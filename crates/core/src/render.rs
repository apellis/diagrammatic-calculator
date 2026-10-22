//! Drawing terms as TikZ and SVG.
//!
//! A word is drawn bottom to top in its canonical layering: one unit of height per level, strand
//! positions one unit apart, strands entering and leaving every piece vertically. The TikZ
//! follows the drawing style of Brundan–Ellis, *Super Kac–Moody 2-categories*
//! (arXiv:1701.04133v2) with core TikZ only: plain strands, solid dots, colour labels under the
//! strands; SVG uses the same geometry.

use crate::context::Context;
use crate::poly::Poly;
use crate::ring::Ring;
use crate::term::{Letter, Object, Term, Word};

/// A drawing primitive in diagram units (`y` grows upwards).
#[derive(Clone, Debug, PartialEq)]
pub enum Prim {
    /// A strand from `(x0, y0)` up to `(x1, y1)`, leaving and arriving vertically.
    Strand {
        x0: f64,
        y0: f64,
        x1: f64,
        y1: f64,
    },
    /// A cup (`up = true`, opening upwards) or cap joining `x0` and `x1` at height `y`.
    Turn {
        x0: f64,
        x1: f64,
        y: f64,
        up: bool,
    },
    Dot {
        x: f64,
        y: f64,
    },
    Box {
        x0: f64,
        y0: f64,
        x1: f64,
        y1: f64,
        label: String,
    },
    /// A colour label under a strand.
    Label {
        x: f64,
        y: f64,
        text: String,
    },
}

/// The primitives of one word, with its width and height, and what each primitive belongs to:
/// the layer of a piece and the strand of a line. Pieces, strands and the objects between
/// heights are what an interactive canvas needs to hit-test and to place new pieces.
#[derive(Clone, Debug, PartialEq)]
pub struct Picture {
    pub prims: Vec<Prim>,
    /// For each primitive: the 0-based index in the word of the layer it draws, if any, and
    /// the strand it is part of, if any.
    pub tags: Vec<Tag>,
    pub pieces: Vec<Piece>,
    pub strands: Vec<StrandInfo>,
    /// The heights between rows of pieces, bottom to top, with the object there.
    pub boundaries: Vec<Boundary>,
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Tag {
    pub layer: Option<usize>,
    pub strand: Option<usize>,
}

/// A drawn piece: one layer of the word.
#[derive(Clone, Debug, PartialEq)]
pub struct Piece {
    /// 0-based index of the layer in the word.
    pub layer: usize,
    /// Bounding box in diagram units.
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
    /// Where the piece's strands enter and leave: positions in the objects below and above.
    pub bottom: usize,
    pub top: usize,
    /// The strands entering and leaving the piece.
    pub inputs: Vec<usize>,
    pub outputs: Vec<usize>,
}

/// A strand: a connected line of the diagram, with the colour and orientation where it starts.
#[derive(Clone, Debug, PartialEq)]
pub struct StrandInfo {
    pub colour: u16,
    pub up: bool,
}

/// A height between two rows of pieces.
#[derive(Clone, Debug, PartialEq)]
pub struct Boundary {
    pub y: f64,
    /// How many layers of the word lie below this height.
    pub below: usize,
    pub object: Object,
}

/// Union-find over strand pieces, so a cap joins the two strands it closes.
struct Strands {
    parent: Vec<usize>,
    info: Vec<StrandInfo>,
}

impl Strands {
    fn new_strand(&mut self, l: Letter) -> usize {
        self.parent.push(self.parent.len());
        self.info.push(StrandInfo {
            colour: l.colour,
            up: l.up,
        });
        self.parent.len() - 1
    }
    fn find(&mut self, mut a: usize) -> usize {
        while self.parent[a] != a {
            self.parent[a] = self.parent[self.parent[a]];
            a = self.parent[a];
        }
        a
    }
    fn join(&mut self, a: usize, b: usize) {
        let (a, b) = (self.find(a), self.find(b));
        if a != b {
            self.parent[a.max(b)] = a.min(b);
        }
    }
}

/// Drawing heights for the layers of `w` in list order. For a word in canonical layering these
/// are the canonical levels. `rank`, when given, is each layer's index in the canonical
/// layering: two consecutive layers then share a height only if they keep their canonical
/// order, so a piece moved above a distant one is drawn above it.
fn draw_levels<R: Ring>(ctx: &Context<R>, w: &Word, rank: Option<&[usize]>) -> Vec<u32> {
    let mut floor: Vec<u32> = vec![0; w.source.len()];
    let mut out: Vec<u32> = Vec::with_capacity(w.layers.len());
    for (k, l) in w.layers.iter().enumerate() {
        let p = l.pos as usize;
        let s = ctx.source_len(l.generator as usize);
        let t = ctx.target_len(l.generator as usize);
        let below = if s > 0 {
            floor[p..p + s].iter().copied().max().unwrap_or(0)
        } else {
            let left = if p > 0 { floor[p - 1] } else { 0 };
            let right = floor.get(p).copied().unwrap_or(0);
            left.max(right)
        };
        let mut lev = below + 1;
        if k > 0 {
            let prev = &w.layers[k - 1];
            let share =
                ctx.relative(prev, l) == Some(false) && rank.is_none_or(|r| r[k - 1] < r[k]);
            lev = lev.max(out[k - 1] + u32::from(!share));
        }
        out.push(lev);
        floor.splice(p..p + s, std::iter::repeat_n(lev, t));
    }
    out
}

pub fn picture<R: Ring>(ctx: &Context<R>, w: &Word) -> Picture {
    picture_ranked(ctx, w, None)
}

/// [`picture`] of a word whose layers are in an order other than the canonical one; `rank`
/// gives each layer's canonical index (see [`draw_levels`]).
pub fn picture_ranked<R: Ring>(ctx: &Context<R>, w: &Word, rank: Option<&[usize]>) -> Picture {
    let levels = draw_levels(ctx, w, rank);
    let mut prims = Vec::new();
    let mut tags = Vec::new();
    let mut pieces = Vec::new();
    let mut st = Strands {
        parent: vec![],
        info: vec![],
    };
    let mut obj = w.source.clone();
    let mut ids: Vec<usize> = obj.iter().map(|&l| st.new_strand(l)).collect();
    for (p, l) in obj.iter().enumerate() {
        let name = &ctx.colours[l.colour as usize].name;
        let text = if l.up {
            name.clone()
        } else {
            format!("{name}*")
        };
        prims.push(Prim::Label {
            x: p as f64,
            y: 0.0,
            text,
        });
        tags.push(Tag {
            layer: None,
            strand: Some(ids[p]),
        });
    }
    let mut boundaries = vec![Boundary {
        y: 0.0,
        below: 0,
        object: obj.clone(),
    }];
    let mut max_width = obj.len();
    let mut i = 0;
    let mut y = 0.0;
    if w.layers.is_empty() {
        for (p, &id) in ids.iter().enumerate() {
            prims.push(Prim::Strand {
                x0: p as f64,
                y0: 0.0,
                x1: p as f64,
                y1: 1.0,
            });
            tags.push(Tag {
                layer: None,
                strand: Some(id),
            });
        }
        y = 1.0;
    }
    while i < w.layers.len() {
        // One height per level, except that odd pieces never share a height: the word reads
        // them bottom to top in list order, and a picture with two at one height is ambiguous.
        let mut j = i;
        let mut has_odd = false;
        while j < w.layers.len() && levels[j] == levels[i] {
            let odd = ctx.layer_odd(&w.layers[j]);
            if odd && has_odd {
                break;
            }
            has_odd |= odd;
            j += 1;
        }
        // positions at the bottom of this level, left to right
        let mut group = Vec::new();
        let mut shift: i64 = 0;
        for (k, l) in w.layers[i..j].iter().enumerate() {
            let g = l.generator as usize;
            let (s, t) = (ctx.source_len(g) as i64, ctx.target_len(g) as i64);
            group.push((
                i + k,
                l,
                (l.pos as i64 - shift) as usize,
                s as usize,
                t as usize,
            ));
            shift += t - s;
        }
        let (y0, y1) = (y, y + 1.0);
        let mut p = 0; // bottom position
        let mut q = 0; // top position
        let mut top_ids: Vec<usize> = Vec::new();
        let line = |prims: &mut Vec<Prim>, tags: &mut Vec<Tag>, prim: Prim, tag: Tag| {
            prims.push(prim);
            tags.push(tag);
        };
        for &(k, l, pb, s, t) in &group {
            while p < pb {
                line(
                    &mut prims,
                    &mut tags,
                    Prim::Strand {
                        x0: p as f64,
                        y0,
                        x1: q as f64,
                        y1,
                    },
                    Tag {
                        layer: None,
                        strand: Some(ids[p]),
                    },
                );
                top_ids.push(ids[p]);
                p += 1;
                q += 1;
            }
            let g = &ctx.pres.generators[l.generator as usize];
            let (xb, xt) = (p as f64, q as f64);
            let inputs: Vec<usize> = ids[p..p + s].to_vec();
            let tag = |strand: Option<usize>| Tag {
                layer: Some(k),
                strand,
            };
            let top_obj = ctx.apply(l, &obj);
            let (bx0, bx1, by0, by1);
            match (g.draw.as_str(), s, t) {
                ("dot", 1, 1) => {
                    line(
                        &mut prims,
                        &mut tags,
                        Prim::Strand {
                            x0: xb,
                            y0,
                            x1: xt,
                            y1,
                        },
                        tag(Some(ids[p])),
                    );
                    let (cx, cy) = ((xb + xt) / 2.0, (y0 + y1) / 2.0);
                    line(
                        &mut prims,
                        &mut tags,
                        Prim::Dot { x: cx, y: cy },
                        tag(Some(ids[p])),
                    );
                    top_ids.push(ids[p]);
                    (bx0, bx1, by0, by1) = (cx - 0.25, cx + 0.25, cy - 0.25, cy + 0.25);
                }
                ("crossing", 2, 2) => {
                    line(
                        &mut prims,
                        &mut tags,
                        Prim::Strand {
                            x0: xb,
                            y0,
                            x1: xt + 1.0,
                            y1,
                        },
                        tag(Some(ids[p])),
                    );
                    line(
                        &mut prims,
                        &mut tags,
                        Prim::Strand {
                            x0: xb + 1.0,
                            y0,
                            x1: xt,
                            y1,
                        },
                        tag(Some(ids[p + 1])),
                    );
                    top_ids.push(ids[p + 1]);
                    top_ids.push(ids[p]);
                    (bx0, bx1, by0, by1) = (xb.min(xt), xb.max(xt) + 1.0, y0, y1);
                }
                ("cup", 0, 2) => {
                    let id = st.new_strand(top_obj[q]);
                    line(
                        &mut prims,
                        &mut tags,
                        Prim::Turn {
                            x0: xt,
                            x1: xt + 1.0,
                            y: y1,
                            up: true,
                        },
                        tag(Some(id)),
                    );
                    top_ids.push(id);
                    top_ids.push(id);
                    (bx0, bx1, by0, by1) = (xt, xt + 1.0, y1 - 0.6, y1);
                }
                ("cap", 2, 0) => {
                    st.join(ids[p], ids[p + 1]);
                    line(
                        &mut prims,
                        &mut tags,
                        Prim::Turn {
                            x0: xb,
                            x1: xb + 1.0,
                            y: y0,
                            up: false,
                        },
                        tag(Some(ids[p])),
                    );
                    (bx0, bx1, by0, by1) = (xb, xb + 1.0, y0, y0 + 0.6);
                }
                _ => {
                    let lo = xb.min(xt) - 0.35;
                    let hi = xb.max(xt) + (s.max(t).max(1) - 1) as f64 + 0.35;
                    let (b0, b1) = (y0 + 0.3, y1 - 0.3);
                    for kk in 0..s {
                        let x = xb + kk as f64;
                        line(
                            &mut prims,
                            &mut tags,
                            Prim::Strand {
                                x0: x,
                                y0,
                                x1: x,
                                y1: b0,
                            },
                            tag(Some(ids[p + kk])),
                        );
                    }
                    for kk in 0..t {
                        let id = st.new_strand(top_obj[q + kk]);
                        let x = xt + kk as f64;
                        line(
                            &mut prims,
                            &mut tags,
                            Prim::Strand {
                                x0: x,
                                y0: b1,
                                x1: x,
                                y1,
                            },
                            tag(Some(id)),
                        );
                        top_ids.push(id);
                    }
                    line(
                        &mut prims,
                        &mut tags,
                        Prim::Box {
                            x0: lo,
                            y0: b0,
                            x1: hi,
                            y1: b1,
                            label: g.name.clone(),
                        },
                        tag(None),
                    );
                    (bx0, bx1, by0, by1) = (lo, hi, b0, b1);
                }
            }
            let outputs = top_ids[top_ids.len() - t..].to_vec();
            pieces.push(Piece {
                layer: k,
                x0: bx0,
                y0: by0,
                x1: bx1,
                y1: by1,
                bottom: p,
                top: q,
                inputs,
                outputs,
            });
            p += s;
            q += t;
        }
        while p < obj.len() {
            line(
                &mut prims,
                &mut tags,
                Prim::Strand {
                    x0: p as f64,
                    y0,
                    x1: q as f64,
                    y1,
                },
                Tag {
                    layer: None,
                    strand: Some(ids[p]),
                },
            );
            top_ids.push(ids[p]);
            p += 1;
            q += 1;
        }
        for l in &w.layers[i..j] {
            obj = ctx.apply(l, &obj);
        }
        ids = top_ids;
        max_width = max_width.max(obj.len());
        y = y1;
        i = j;
        boundaries.push(Boundary {
            y,
            below: j,
            object: obj.clone(),
        });
    }
    // strands joined by caps take one number; strands are numbered in order of appearance
    let mut number: Vec<Option<usize>> = vec![None; st.parent.len()];
    let mut strands = Vec::new();
    let mut renumber = |st: &mut Strands, id: usize| {
        let r = st.find(id);
        *number[r].get_or_insert_with(|| {
            strands.push(st.info[r].clone());
            strands.len() - 1
        })
    };
    for t in &mut tags {
        if let Some(id) = t.strand {
            t.strand = Some(renumber(&mut st, id));
        }
    }
    for pc in &mut pieces {
        for id in pc.inputs.iter_mut().chain(pc.outputs.iter_mut()) {
            *id = renumber(&mut st, *id);
        }
    }
    Picture {
        prims,
        tags,
        pieces,
        strands,
        boundaries,
        width: max_width.saturating_sub(1) as f64,
        height: y,
    }
}

fn num(x: f64) -> String {
    let s = format!("{:.2}", x);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.into() }
}

/// One word as a `tikzpicture`.
pub fn tikz_word<R: Ring>(ctx: &Context<R>, w: &Word) -> String {
    let pic = picture(ctx, w);
    let mut s = String::from(
        "\\begin{tikzpicture}[x=0.5cm,y=0.5cm,baseline={([yshift=-.5ex]current bounding box.center)}]\n",
    );
    for p in &pic.prims {
        let line = match p {
            Prim::Strand { x0, y0, x1, y1 } if x0 == x1 => {
                format!(
                    "\\draw ({},{}) -- ({},{});",
                    num(*x0),
                    num(*y0),
                    num(*x1),
                    num(*y1)
                )
            }
            Prim::Strand { x0, y0, x1, y1 } => format!(
                "\\draw ({},{}) to[out=90,in=-90] ({},{});",
                num(*x0),
                num(*y0),
                num(*x1),
                num(*y1)
            ),
            Prim::Turn { x0, x1, y, up } => {
                let a = if *up { -90 } else { 90 };
                format!(
                    "\\draw ({},{}) to[out={a},in={a}] ({},{});",
                    num(*x0),
                    num(*y),
                    num(*x1),
                    num(*y)
                )
            }
            Prim::Dot { x, y } => format!("\\fill ({},{}) circle (2pt);", num(*x), num(*y)),
            Prim::Box {
                x0,
                y0,
                x1,
                y1,
                label,
            } => format!(
                "\\draw ({},{}) rectangle ({},{}); \\node at ({},{}) {{$\\scriptstyle {}$}};",
                num(*x0),
                num(*y0),
                num(*x1),
                num(*y1),
                num((x0 + x1) / 2.0),
                num((y0 + y1) / 2.0),
                label
            ),
            Prim::Label { x, y, text } => {
                format!(
                    "\\node[below] at ({},{}) {{$\\scriptstyle {}$}};",
                    num(*x),
                    num(*y),
                    text
                )
            }
        };
        s.push_str("  ");
        s.push_str(&line);
        s.push('\n');
    }
    s.push_str("\\end{tikzpicture}");
    s
}

/// The coefficients of a term as signs and printed absolute values.
fn signed_terms<R: Ring>(
    ctx: &Context<R>,
    t: &Term<Poly<R::Elem>>,
) -> Vec<(bool, Option<String>, Word)> {
    t.terms
        .iter()
        .map(|(w, c)| {
            let neg = ctx.coeff.is_negative(c);
            let a = if neg { ctx.coeff.neg(c) } else { c.clone() };
            let cs = if ctx.coeff.is_one(&a) {
                None
            } else if ctx.coeff.needs_parens(&a) {
                Some(format!("({})", ctx.coeff.fmt_elem(&a)))
            } else {
                Some(ctx.coeff.fmt_elem(&a))
            };
            (neg, cs, w.clone())
        })
        .collect()
}

/// A term as a LaTeX math expression of `tikzpicture`s.
pub fn tikz<R: Ring>(ctx: &Context<R>, t: &Term<Poly<R::Elem>>) -> String {
    if t.is_zero() {
        return "0".into();
    }
    let mut s = String::new();
    for (k, (neg, c, w)) in signed_terms(ctx, t).into_iter().enumerate() {
        if k == 0 {
            if neg {
                s.push('-');
            }
        } else {
            s.push_str(if neg { "\n-\n" } else { "\n+\n" });
        }
        if let Some(c) = c {
            s.push_str(&c);
            s.push_str("\\;");
        }
        s.push_str(&tikz_word(ctx, &w));
    }
    s
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// A term as a standalone SVG.
pub fn svg<R: Ring>(ctx: &Context<R>, t: &Term<Poly<R::Elem>>) -> String {
    const U: f64 = 24.0;
    const M: f64 = 12.0;
    let items = signed_terms(ctx, t);
    let pics: Vec<Picture> = items.iter().map(|(_, _, w)| picture(ctx, w)).collect();
    let height = pics.iter().map(|p| p.height).fold(1.0, f64::max);
    let total_h = height * U + 2.0 * M + 14.0;
    let base = M + height * U; // y of diagram height 0
    let mut body = String::new();
    let mut x = M;
    if items.is_empty() {
        body.push_str(&format!(
            "<text x=\"{x}\" y=\"{}\" font-size=\"14\">0</text>",
            num(base - height * U / 2.0)
        ));
        x += 12.0;
    }
    for (k, ((neg, c, _), pic)) in items.iter().zip(&pics).enumerate() {
        let mut lead = String::new();
        if k > 0 || *neg {
            lead.push_str(if *neg { "\u{2212} " } else { "+ " });
        }
        if let Some(c) = c {
            lead.push_str(c);
        }
        if !lead.is_empty() {
            let mid = base - height * U / 2.0 + 5.0;
            body.push_str(&format!(
                "<text x=\"{}\" y=\"{}\" font-size=\"14\" font-family=\"serif\">{}</text>\n",
                num(x),
                num(mid),
                esc(lead.trim_end())
            ));
            x += 8.0 * lead.chars().count() as f64 + 6.0;
        }
        let off = (height - pic.height) / 2.0;
        let px = |dx: f64| num(x + dx * U);
        let py = |dy: f64| num(base - (dy + off) * U);
        for p in &pic.prims {
            let line = match p {
                Prim::Strand { x0, y0, x1, y1 } => {
                    let ym = (y0 + y1) / 2.0;
                    format!(
                        "<path d=\"M {} {} C {} {} {} {} {} {}\"/>",
                        px(*x0),
                        py(*y0),
                        px(*x0),
                        py(ym),
                        px(*x1),
                        py(ym),
                        px(*x1),
                        py(*y1)
                    )
                }
                Prim::Turn { x0, x1, y, up } => {
                    let d = if *up { -0.6 } else { 0.6 };
                    format!(
                        "<path d=\"M {} {} C {} {} {} {} {} {}\"/>",
                        px(*x0),
                        py(*y),
                        px(*x0),
                        py(y + d),
                        px(*x1),
                        py(y + d),
                        px(*x1),
                        py(*y)
                    )
                }
                Prim::Dot { x, y } => format!(
                    "<circle cx=\"{}\" cy=\"{}\" r=\"3\" fill=\"currentColor\"/>",
                    px(*x),
                    py(*y)
                ),
                Prim::Box {
                    x0,
                    y0,
                    x1,
                    y1,
                    label,
                } => format!(
                    "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"none\"/><text x=\"{}\" y=\"{}\" font-size=\"11\" text-anchor=\"middle\" stroke=\"none\" fill=\"currentColor\">{}</text>",
                    px(*x0),
                    py(*y1),
                    num((x1 - x0) * U),
                    num((y1 - y0) * U),
                    px((x0 + x1) / 2.0),
                    num(base - ((y0 + y1) / 2.0 + off) * U + 4.0),
                    esc(label)
                ),
                Prim::Label { x, y, text } => format!(
                    "<text x=\"{}\" y=\"{}\" font-size=\"11\" font-style=\"italic\" text-anchor=\"middle\" stroke=\"none\" fill=\"currentColor\">{}</text>",
                    px(*x),
                    num(base - (y + off) * U + 13.0),
                    esc(text)
                ),
            };
            body.push_str(&line);
            body.push('\n');
        }
        x += pic.width * U + 18.0;
    }
    let width = x - 6.0 + M;
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.4\">\n{body}</svg>\n",
        w = num(width),
        h = num(total_h)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtin;
    use crate::context::ContextSpec;
    use crate::ring::Rationals;

    #[test]
    fn crossing_and_dot() {
        let spec = ContextSpec::parse("klr type=A2 nu=i+j").unwrap();
        let ctx = Context::new(builtin::presentation("klr").unwrap(), spec, Rationals).unwrap();
        let t = ctx.parse_term("x_2 psi_1 e(ij) - 2 e(ij)").unwrap();
        let tz = tikz(&ctx, &t);
        assert!(tz.contains("\\draw (0,0) to[out=90,in=-90] (1,1);"), "{tz}");
        assert!(tz.contains("\\fill (1,1.5) circle (2pt);"), "{tz}");
        assert!(
            tz.contains("\\node[below] at (0,0) {$\\scriptstyle i$};"),
            "{tz}"
        );
        assert!(tz.starts_with("-2\\;"), "{tz}");
        let s = svg(&ctx, &t);
        assert!(s.starts_with("<svg") && s.contains("<circle"), "{s}");
    }

    #[test]
    fn drawing_levels_are_canonical_levels() {
        for (pres, ctx_text, terms) in [
            (
                "klr",
                "klr type=A3 nu=2i+j+k",
                "psi_1 psi_2 psi_3 x_1 psi_2 x_4 psi_1 e(ijik) + x_2 x_3 psi_3 psi_1 e(iijk)",
            ),
            (
                "odd-nilhecke",
                "odd-nilhecke nu=4",
                "x_1 x_3 psi_2 x_4 psi_1 psi_3 x_2 + psi_1 psi_3 x_1 x_4",
            ),
        ] {
            let spec = ContextSpec::parse(ctx_text).unwrap();
            let ctx = Context::new(builtin::presentation(pres).unwrap(), spec, Rationals).unwrap();
            let t = ctx.parse_term(terms).unwrap();
            for (w, _) in &t.terms {
                assert_eq!(
                    draw_levels(&ctx, w, None),
                    ctx.levels(w),
                    "{}",
                    ctx.fmt_word(w)
                );
                let ident: Vec<usize> = (0..w.layers.len()).collect();
                assert_eq!(picture_ranked(&ctx, w, Some(&ident)), picture(&ctx, w));
            }
        }
    }
}
