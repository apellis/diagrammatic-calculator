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
    Strand { x0: f64, y0: f64, x1: f64, y1: f64 },
    /// A cup (`up = true`, opening upwards) or cap joining `x0` and `x1` at height `y`.
    Turn { x0: f64, x1: f64, y: f64, up: bool },
    /// A dot, or a dot power drawn as one dot labelled with its exponent (`power` above 1).
    Dot { x: f64, y: f64, power: usize },
    Box {
        x0: f64,
        y0: f64,
        x1: f64,
        y1: f64,
        label: String,
    },
    /// A colour label under a strand.
    Label { x: f64, y: f64, text: String },
    /// An arrowhead at `(x, y)` pointing along `(dx, dy)`: the orientation of a strand.
    Arrow { x: f64, y: f64, dx: f64, dy: f64 },
    /// Text centred at `(x, y)`: the colour of a strand that does not start at the bottom, or
    /// the weight of the rightmost region.
    Note { x: f64, y: f64, text: String },
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

/// The least power of a dot drawn as one dot labelled with its exponent.
pub const DOT_POWER: usize = 4;

/// The dot powers of `w` drawn as one dot: for each layer, the first layer of its power when it is
/// a later dot of one. A power is a run of at least [`DOT_POWER`] even dots of one generator,
/// one after another along a strand with nothing between them. Odd dots are left alone: their
/// heights against the other odd pieces carry the word's sign.
fn dot_powers<R: Ring>(ctx: &Context<R>, w: &Word) -> Vec<Option<usize>> {
    let parities = ctx.layer_parities(w);
    // for each position of the current object: the first dot of the run its strand ends in
    let mut last: Vec<Option<usize>> = vec![None; w.source.len()];
    let mut head: Vec<usize> = (0..w.layers.len()).collect();
    let mut count = vec![1usize; w.layers.len()];
    for (k, l) in w.layers.iter().enumerate() {
        let g = l.generator as usize;
        let p = l.pos as usize;
        let (s, t) = (ctx.source_len(g), ctx.target_len(g));
        if ctx.pres.generators[g].draw == "dot" && s == 1 && t == 1 && !parities[k] {
            match last[p] {
                Some(h) if w.layers[h].generator == l.generator => {
                    head[k] = h;
                    count[h] += 1;
                }
                _ => last[p] = Some(k),
            }
        } else {
            last.splice(p..p + s, std::iter::repeat_n(None, t));
        }
    }
    (0..w.layers.len())
        .map(|k| (head[k] != k && count[head[k]] >= DOT_POWER).then_some(head[k]))
        .collect()
}

/// [`picture`] of a word whose layers are in an order other than the canonical one; `rank`
/// gives each layer's canonical index (see [`draw_levels`]).
///
/// A dot power of at least [`DOT_POWER`] is drawn as its first dot, labelled with the exponent;
/// the other dots of the power are pieces in the same place, so each layer still has a piece.
pub fn picture_ranked<R: Ring>(ctx: &Context<R>, w: &Word, rank: Option<&[usize]>) -> Picture {
    let absorbed = dot_powers(ctx, w);
    if absorbed.iter().all(Option::is_none) {
        return picture_layers(ctx, w, rank);
    }
    // draw the word without the later dots of each power: they act on no object
    let keep: Vec<usize> = (0..w.layers.len())
        .filter(|&k| absorbed[k].is_none())
        .collect();
    let shown = Word {
        source: w.source.clone(),
        layers: keep.iter().map(|&k| w.layers[k].clone()).collect(),
    };
    let shown_rank: Option<Vec<usize>> = rank.map(|r| keep.iter().map(|&k| r[k]).collect());
    let mut pic = picture_layers(ctx, &shown, shown_rank.as_deref());
    for t in &mut pic.tags {
        if let Some(l) = &mut t.layer {
            *l = keep[*l];
        }
    }
    for pc in &mut pic.pieces {
        pc.layer = keep[pc.layer];
    }
    for (p, t) in pic.prims.iter_mut().zip(&pic.tags) {
        if let (Prim::Dot { power, .. }, Some(l)) = (p, t.layer) {
            *power = 1 + absorbed.iter().filter(|&&h| h == Some(l)).count();
        }
    }
    for (k, h) in absorbed.iter().enumerate() {
        if let Some(h) = h {
            let first = pic.pieces.iter().find(|pc| pc.layer == *h).cloned();
            if let Some(mut pc) = first {
                pc.layer = k;
                pic.pieces.push(pc);
            }
        }
    }
    pic
}

/// [`picture_ranked`] with every layer drawn.
fn picture_layers<R: Ring>(ctx: &Context<R>, w: &Word, rank: Option<&[usize]>) -> Picture {
    let levels = draw_levels(ctx, w, rank);
    let parities = ctx.layer_parities(w);
    let mut prims = Vec::new();
    let mut tags = Vec::new();
    // For each primitive that draws a line: whether the strand points up there (for a turn,
    // its left end).
    let mut ups: Vec<Option<bool>> = Vec::new();
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
        ups.push(None);
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
            ups.push(Some(obj[p].up));
        }
        y = 1.0;
    }
    while i < w.layers.len() {
        // One height per level, except that odd pieces never share a height: the word reads
        // them bottom to top in list order, and a picture with two at one height is ambiguous.
        let mut j = i;
        let mut has_odd = false;
        while j < w.layers.len() && levels[j] == levels[i] {
            let odd = parities[j];
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
        let line = |prims: &mut Vec<Prim>,
                    tags: &mut Vec<Tag>,
                    ups: &mut Vec<Option<bool>>,
                    prim: Prim,
                    tag: Tag,
                    up: Option<bool>| {
            prims.push(prim);
            tags.push(tag);
            ups.push(up);
        };
        for &(k, l, pb, s, t) in &group {
            while p < pb {
                line(
                    &mut prims,
                    &mut tags,
                    &mut ups,
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
                    Some(obj[p].up),
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
                        &mut ups,
                        Prim::Strand {
                            x0: xb,
                            y0,
                            x1: xt,
                            y1,
                        },
                        tag(Some(ids[p])),
                        Some(obj[p].up),
                    );
                    let (cx, cy) = ((xb + xt) / 2.0, (y0 + y1) / 2.0);
                    line(
                        &mut prims,
                        &mut tags,
                        &mut ups,
                        Prim::Dot {
                            x: cx,
                            y: cy,
                            power: 1,
                        },
                        tag(Some(ids[p])),
                        None,
                    );
                    top_ids.push(ids[p]);
                    (bx0, bx1, by0, by1) = (cx - 0.25, cx + 0.25, cy - 0.25, cy + 0.25);
                }
                ("crossing", 2, 2) => {
                    line(
                        &mut prims,
                        &mut tags,
                        &mut ups,
                        Prim::Strand {
                            x0: xb,
                            y0,
                            x1: xt + 1.0,
                            y1,
                        },
                        tag(Some(ids[p])),
                        Some(obj[p].up),
                    );
                    line(
                        &mut prims,
                        &mut tags,
                        &mut ups,
                        Prim::Strand {
                            x0: xb + 1.0,
                            y0,
                            x1: xt,
                            y1,
                        },
                        tag(Some(ids[p + 1])),
                        Some(obj[p + 1].up),
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
                        &mut ups,
                        Prim::Turn {
                            x0: xt,
                            x1: xt + 1.0,
                            y: y1,
                            up: true,
                        },
                        tag(Some(id)),
                        Some(top_obj[q].up),
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
                        &mut ups,
                        Prim::Turn {
                            x0: xb,
                            x1: xb + 1.0,
                            y: y0,
                            up: false,
                        },
                        tag(Some(ids[p])),
                        Some(obj[p].up),
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
                            &mut ups,
                            Prim::Strand {
                                x0: x,
                                y0,
                                x1: x,
                                y1: b0,
                            },
                            tag(Some(ids[p + kk])),
                            Some(obj[p + kk].up),
                        );
                    }
                    for kk in 0..t {
                        let id = st.new_strand(top_obj[q + kk]);
                        let x = xt + kk as f64;
                        line(
                            &mut prims,
                            &mut tags,
                            &mut ups,
                            Prim::Strand {
                                x0: x,
                                y0: b1,
                                x1: x,
                                y1,
                            },
                            tag(Some(id)),
                            Some(top_obj[q + kk].up),
                        );
                        top_ids.push(id);
                    }
                    line(
                        &mut prims,
                        &mut tags,
                        &mut ups,
                        Prim::Box {
                            x0: lo,
                            y0: b0,
                            x1: hi,
                            y1: b1,
                            label: g.name.clone(),
                        },
                        tag(None),
                        None,
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
                &mut ups,
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
                Some(obj[p].up),
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
    let mut width = max_width.saturating_sub(1) as f64;
    mark_strands(ctx, &mut st, &mut prims, &mut tags, &ups);
    if let Some(mu) = &ctx.mu {
        let x = if max_width == 0 { 0.5 } else { width + 0.8 };
        prims.push(Prim::Note {
            x,
            y: y / 2.0,
            text: ctx.fmt_region(mu),
        });
        tags.push(Tag::default());
        width = x + 0.5;
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
        width,
        height: y,
    }
}

/// A point of the strand curve from `(x0, y0)` to `(x1, y1)` (a cubic leaving and arriving
/// vertically) at parameter `t`, with the direction of travel there.
fn on_strand(x0: f64, y0: f64, x1: f64, y1: f64, t: f64) -> (f64, f64, f64, f64) {
    let ym = (y0 + y1) / 2.0;
    let u = 1.0 - t;
    let x = u * u * u * x0 + 3.0 * u * u * t * x0 + 3.0 * u * t * t * x1 + t * t * t * x1;
    let y = u * u * u * y0 + 3.0 * u * u * t * ym + 3.0 * u * t * t * ym + t * t * t * y1;
    let dx = 6.0 * u * t * (x1 - x0);
    let dy = 3.0 * u * u * (ym - y0) + 3.0 * t * t * (y1 - ym);
    let n = (dx * dx + dy * dy).sqrt();
    (x, y, dx / n, dy / n)
}

/// One arrowhead on each strand of an oriented colour, and the colour of each strand that does
/// not start at the bottom. A strand's mark goes on its first plain vertical stretch, else on a
/// stretch with a dot (below the dot), else on a crossing, else on the end of a cup or cap.
fn mark_strands<R: Ring>(
    ctx: &Context<R>,
    st: &mut Strands,
    prims: &mut Vec<Prim>,
    tags: &mut Vec<Tag>,
    ups: &[Option<bool>],
) {
    let mut labelled = std::collections::BTreeSet::new();
    let mut best: std::collections::BTreeMap<usize, (u8, usize)> = Default::default();
    for (k, (p, t)) in prims.iter().zip(tags.iter()).enumerate() {
        let Some(id) = t.strand else { continue };
        let r = st.find(id);
        let rank = match p {
            Prim::Label { .. } => {
                labelled.insert(r);
                continue;
            }
            Prim::Strand { x0, x1, y0, y1 } if x0 == x1 && (y1 - y0) > 0.9 => {
                u8::from(t.layer.is_some())
            }
            Prim::Strand { x0, x1, .. } if x0 != x1 => 2,
            Prim::Strand { .. } => 3,
            Prim::Turn { .. } => 4,
            _ => continue,
        };
        if ups[k].is_some() && best.get(&r).is_none_or(|b| rank < b.0) {
            best.insert(r, (rank, k));
        }
    }
    for (&r, &(rank, k)) in &best {
        let up = ups[k] == Some(true);
        let (x, y, dx, dy, nx, ny) = match prims[k] {
            Prim::Strand { x0, y0, x1, y1 } => {
                let t = match rank {
                    1 => 0.2,
                    2 => 0.22,
                    _ => 0.5,
                };
                let (x, y, dx, dy) = on_strand(x0, y0, x1, y1, t);
                let s = if up { 1.0 } else { -1.0 };
                (x, y, s * dx, s * dy, x - 0.32, y)
            }
            Prim::Turn { x0, x1, y, up: cup } => {
                let (x, d) = ((x0 + x1) / 2.0, if cup { -0.45 } else { 0.45 });
                // along a cup whose left end points up the strand runs right to left
                let right = up != cup;
                let dx = if right { 1.0 } else { -1.0 };
                (x, y + d, dx, 0.0, x, y + d + d * 0.6)
            }
            _ => continue,
        };
        let info = &st.info[r];
        let colour = &ctx.colours[info.colour as usize];
        let tag = tags[k];
        if colour.oriented {
            prims.push(Prim::Arrow { x, y, dx, dy });
            tags.push(Tag {
                layer: None,
                strand: tag.strand,
            });
        }
        if !labelled.contains(&r) {
            prims.push(Prim::Note {
                x: nx,
                y: ny,
                text: colour.name.clone(),
            });
            tags.push(Tag {
                layer: None,
                strand: tag.strand,
            });
        }
    }
}

/// The corners of an arrowhead: tip, then the two back corners.
pub fn arrowhead(x: f64, y: f64, dx: f64, dy: f64) -> [(f64, f64); 3] {
    let (l, h) = (0.13, 0.1);
    [
        (x + dx * l, y + dy * l),
        (x - dx * l - dy * h, y - dy * l + dx * h),
        (x - dx * l + dy * h, y - dy * l - dx * h),
    ]
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
            Prim::Dot { x, y, power } => {
                let mut d = format!("\\fill ({},{}) circle (2pt);", num(*x), num(*y));
                if *power > 1 {
                    d.push_str(&format!(
                        " \\node[right=2.5pt,inner sep=0pt] at ({},{}) {{$\\scriptstyle {power}$}};",
                        num(*x),
                        num(*y)
                    ));
                }
                d
            }
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
            Prim::Arrow { x, y, dx, dy } => {
                let [a, b, c] = arrowhead(*x, *y, *dx, *dy);
                format!(
                    "\\fill ({},{}) -- ({},{}) -- ({},{}) -- cycle;",
                    num(a.0),
                    num(a.1),
                    num(b.0),
                    num(b.1),
                    num(c.0),
                    num(c.1)
                )
            }
            Prim::Note { x, y, text } => {
                format!(
                    "\\node at ({},{}) {{$\\scriptstyle {}$}};",
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

/// Text as typeset: a minus sign (U+2212) rather than a hyphen before a digit, as in a region
/// weight `−2` (`docs/conventions.md`, Display). TikZ needs no such change: its labels are in math
/// mode.
fn minus_signs(t: &str) -> String {
    let mut out = String::with_capacity(t.len());
    let mut prev: Option<char> = None;
    let mut chars = t.chars().peekable();
    while let Some(c) = chars.next() {
        let word = prev.is_some_and(|p| p.is_alphanumeric() || p == '_');
        if c == '-' && !word && chars.peek().is_some_and(|d| d.is_ascii_digit()) {
            out.push('\u{2212}');
        } else {
            out.push(c);
        }
        prev = Some(c);
    }
    out
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
        for (p, tag) in pic.prims.iter().zip(&pic.tags) {
            // a strand's lines, dots, arrowheads and label carry its colour's class (`k0` for the
            // context's first colour); a page's style sheet may colour them, plain SVG stays black
            let k = tag
                .strand
                .and_then(|s| pic.strands.get(s))
                .map(|s| format!(" class=\"k{}\"", s.colour))
                .unwrap_or_default();
            let line = match p {
                Prim::Strand { x0, y0, x1, y1 } => {
                    let ym = (y0 + y1) / 2.0;
                    format!(
                        "<path{k} d=\"M {} {} C {} {} {} {} {} {}\"/>",
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
                        "<path{k} d=\"M {} {} C {} {} {} {} {} {}\"/>",
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
                Prim::Dot {
                    x: cx,
                    y: cy,
                    power,
                } => {
                    let mut d = format!(
                        "<circle{k} cx=\"{}\" cy=\"{}\" r=\"3\" fill=\"currentColor\"/>",
                        px(*cx),
                        py(*cy)
                    );
                    if *power > 1 {
                        d.push_str(&format!(
                            "<text{k} x=\"{}\" y=\"{}\" font-size=\"10\" stroke=\"none\" fill=\"currentColor\">{power}</text>",
                            num(x + cx * U + 5.0),
                            num(base - (cy + off) * U + 3.5)
                        ));
                    }
                    d
                }
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
                    "<text{k} x=\"{}\" y=\"{}\" font-size=\"11\" font-style=\"italic\" text-anchor=\"middle\" stroke=\"none\" fill=\"currentColor\">{}</text>",
                    px(*x),
                    num(base - (y + off) * U + 13.0),
                    esc(text)
                ),
                Prim::Arrow { x, y, dx, dy } => {
                    let pts: Vec<String> = arrowhead(*x, *y, *dx, *dy)
                        .iter()
                        .map(|(a, b)| format!("{},{}", px(*a), py(*b)))
                        .collect();
                    format!(
                        "<polygon{k} points=\"{}\" fill=\"currentColor\" stroke=\"none\"/>",
                        pts.join(" ")
                    )
                }
                Prim::Note { x, y, text } => format!(
                    "<text{k} x=\"{}\" y=\"{}\" font-size=\"11\" font-style=\"italic\" text-anchor=\"middle\" stroke=\"none\" fill=\"currentColor\">{}</text>",
                    px(*x),
                    num(base - (y + off) * U + 4.0),
                    esc(&minus_signs(text))
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
        // strands carry their colour's class: x_2 is on the i strand (colour 0) after the crossing
        assert!(
            s.contains("<path class=\"k0\"") && s.contains("<path class=\"k1\""),
            "{s}"
        );
        assert!(s.contains("<circle class=\"k0\""), "{s}");
        assert!(s.contains("<text class=\"k0\""), "{s}");
    }

    #[test]
    fn oriented_strands_bubbles_and_regions() {
        let spec = ContextSpec::parse("kl type=A1 mu=-2").unwrap();
        let ctx = Context::new(builtin::presentation("kl").unwrap(), spec, Rationals).unwrap();
        // a clockwise bubble with one dot beside a downward strand
        let t = ctx.parse_term("cw_{i,1} e(i*)").unwrap();
        let pic = picture(&ctx, &t.terms[0].0);
        let arrows: Vec<(f64, f64)> = pic
            .prims
            .iter()
            .filter_map(|p| match p {
                Prim::Arrow { dx, dy, .. } => Some((*dx, *dy)),
                _ => None,
            })
            .collect();
        // the strand points down; the bubble runs up its left side
        assert_eq!(arrows, vec![(0.0, -1.0), (0.0, 1.0)], "{:?}", pic.prims);
        let notes: Vec<&str> = pic
            .prims
            .iter()
            .filter_map(|p| match p {
                Prim::Note { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(notes, vec!["i", "-2"]);
        // the SVG writes the weight with a minus sign
        let s = svg(&ctx, &t);
        assert!(
            s.contains(">\u{2212}2</text>") && !s.contains(">-2<"),
            "{s}"
        );
        assert_eq!(minus_signs("2i-1 -3 x-2"), "2i-1 \u{2212}3 x-2");
        let tz = tikz(&ctx, &t);
        assert!(tz.contains("\\fill (0,0.37) -- "), "{tz}");
        // an unoriented presentation draws no arrows
        let spec = ContextSpec::parse("klr type=A1 nu=2i").unwrap();
        let ctx = Context::new(builtin::presentation("klr").unwrap(), spec, Rationals).unwrap();
        let t = ctx.parse_term("psi_1 e(ii)").unwrap();
        let pic = picture(&ctx, &t.terms[0].0);
        assert!(
            !pic.prims
                .iter()
                .any(|p| matches!(p, Prim::Arrow { .. } | Prim::Note { .. }))
        );
    }

    #[test]
    fn dot_powers_above_three_are_one_labelled_dot() {
        let ctx = |text: &str, pres: &str| {
            let spec = ContextSpec::parse(text).unwrap();
            Context::new(builtin::presentation(pres).unwrap(), spec, Rationals).unwrap()
        };
        let dots = |pic: &Picture| -> Vec<usize> {
            pic.prims
                .iter()
                .filter_map(|p| match p {
                    Prim::Dot { power, .. } => Some(*power),
                    _ => None,
                })
                .collect()
        };
        let c = ctx("nilhecke nu=3", "nilhecke");
        // a power of 3 stays three dots; 5 is one dot labelled 5, at one height
        let w = |t: &str| c.parse_term(t).unwrap().terms[0].0.clone();
        assert_eq!(dots(&picture(&c, &w("x_1^3"))), vec![1, 1, 1]);
        let pic = picture(&c, &w("x_1^5 x_2"));
        assert_eq!(dots(&pic), vec![5, 1]);
        assert_eq!(pic.height, 1.0);
        // every layer keeps a piece, the power's all in one place
        assert_eq!(pic.pieces.len(), 6);
        let boxes: Vec<_> = pic
            .pieces
            .iter()
            .filter(|pc| pc.x0 < 0.0)
            .map(|pc| (pc.x0, pc.y0))
            .collect();
        assert_eq!(boxes.len(), 5);
        assert!(boxes.iter().all(|b| *b == boxes[0]));
        // dots separated by a crossing on their strand are two powers
        let pic = picture(&c, &w("x_1^4 psi_1 x_1^4"));
        assert_eq!(dots(&pic), vec![4, 4]);
        assert!(svg(&c, &c.parse_term("x_1^4").unwrap()).contains(">4</text>"));
        let tz = tikz(&c, &c.parse_term("x_1^4").unwrap());
        assert_eq!(tz.matches("circle").count(), 1, "{tz}");
        assert!(tz.contains("{$\\scriptstyle 4$}"), "{tz}");
        // odd dots stay separate: their heights against other odd pieces carry the sign
        let c = ctx("be2 type=A1 odd=i mu=-1", "be2");
        let t = c.parse_term("x_1^4 e(i)").unwrap();
        assert!(c.layer_parities(&t.terms[0].0).iter().all(|&o| o));
        assert_eq!(dots(&picture(&c, &t.terms[0].0)), vec![1, 1, 1, 1]);
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
