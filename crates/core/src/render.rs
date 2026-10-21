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
use crate::term::{Term, Word};

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

/// The primitives of one word, with its width and height.
#[derive(Clone, Debug, PartialEq)]
pub struct Picture {
    pub prims: Vec<Prim>,
    pub width: f64,
    pub height: f64,
}

pub fn picture<R: Ring>(ctx: &Context<R>, w: &Word) -> Picture {
    let levels = ctx.levels(w);
    let mut prims = Vec::new();
    let mut obj = w.source.clone();
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
    }
    let mut max_width = obj.len();
    let mut i = 0;
    let mut y = 0.0;
    if w.layers.is_empty() {
        for p in 0..obj.len() {
            prims.push(Prim::Strand {
                x0: p as f64,
                y0: 0.0,
                x1: p as f64,
                y1: 1.0,
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
        let mut pieces = Vec::new();
        let mut shift: i64 = 0;
        for l in &w.layers[i..j] {
            let g = l.generator as usize;
            let (s, t) = (ctx.source_len(g) as i64, ctx.target_len(g) as i64);
            pieces.push((l, (l.pos as i64 - shift) as usize, s as usize, t as usize));
            shift += t - s;
        }
        let (y0, y1) = (y, y + 1.0);
        let mut p = 0; // bottom position
        let mut q = 0; // top position
        for &(l, pb, s, t) in &pieces {
            while p < pb {
                prims.push(Prim::Strand {
                    x0: p as f64,
                    y0,
                    x1: q as f64,
                    y1,
                });
                p += 1;
                q += 1;
            }
            let g = &ctx.pres.generators[l.generator as usize];
            let (xb, xt) = (p as f64, q as f64);
            match (g.draw.as_str(), s, t) {
                ("dot", 1, 1) => {
                    prims.push(Prim::Strand {
                        x0: xb,
                        y0,
                        x1: xt,
                        y1,
                    });
                    prims.push(Prim::Dot {
                        x: (xb + xt) / 2.0,
                        y: (y0 + y1) / 2.0,
                    });
                }
                ("crossing", 2, 2) => {
                    prims.push(Prim::Strand {
                        x0: xb,
                        y0,
                        x1: xt + 1.0,
                        y1,
                    });
                    prims.push(Prim::Strand {
                        x0: xb + 1.0,
                        y0,
                        x1: xt,
                        y1,
                    });
                }
                ("cup", 0, 2) => prims.push(Prim::Turn {
                    x0: xt,
                    x1: xt + 1.0,
                    y: y1,
                    up: true,
                }),
                ("cap", 2, 0) => prims.push(Prim::Turn {
                    x0: xb,
                    x1: xb + 1.0,
                    y: y0,
                    up: false,
                }),
                _ => {
                    let lo = xb.min(xt) - 0.35;
                    let hi = xb.max(xt) + (s.max(t).max(1) - 1) as f64 + 0.35;
                    let (by0, by1) = (y0 + 0.3, y1 - 0.3);
                    for k in 0..s {
                        let x = xb + k as f64;
                        prims.push(Prim::Strand {
                            x0: x,
                            y0,
                            x1: x,
                            y1: by0,
                        });
                    }
                    for k in 0..t {
                        let x = xt + k as f64;
                        prims.push(Prim::Strand {
                            x0: x,
                            y0: by1,
                            x1: x,
                            y1,
                        });
                    }
                    prims.push(Prim::Box {
                        x0: lo,
                        y0: by0,
                        x1: hi,
                        y1: by1,
                        label: g.name.clone(),
                    });
                }
            }
            p += s;
            q += t;
        }
        while p < obj.len() {
            prims.push(Prim::Strand {
                x0: p as f64,
                y0,
                x1: q as f64,
                y1,
            });
            p += 1;
            q += 1;
        }
        for l in &w.layers[i..j] {
            obj = ctx.apply(l, &obj);
        }
        max_width = max_width.max(obj.len());
        y = y1;
        i = j;
    }
    Picture {
        prims,
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
}
