//! Bounded complexes over an additive category given by an evaluator (`docs/complexes.md`).
//!
//! An object of a complex is a finite direct sum of shifted indecomposables `X⟨s⟩` at each
//! position `p ∈ ℤⁿ`; a differential along axis `a` is a matrix of morphisms from position `p`
//! to `p ± e_a` (rows index the target, columns the source). Nothing here depends on the algebra:
//! a family enters through [`Additive`], which supplies objects, morphisms, composition, a test for
//! zero, and, for linear algebra, a basis of each graded piece of `Hom(X, Y)`.
//!
//! Grading: `X⟨s⟩` has its generator in degree `s` (`(M⟨s⟩)_d = M_{d−s}`), so a morphism `f` of
//! degree `deg f` gives a degree-zero map `X⟨s⟩ → Y⟨t⟩` exactly when `deg f = s − t`.
//!
//! Homology is computed by evaluating at test objects `T`: the degree-`d` part of `Hom(T, C^p)` is
//! `⊕_i Hom(T, X_i)_{d − s_i}`, a finite-dimensional space, and the differential acts by
//! postcomposition. For projective modules over an algebra `A` with test objects the
//! indecomposable projectives `e A`, this is `C^p e` in degree `d`, so the results determine the
//! homology as graded modules.

use crate::error::{Error, Result};
use crate::linalg::{Mat, Reducer};
use crate::ring::Ring;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

/// The source, target, and degree of a homogeneous morphism.
pub type MorEnds<O> = (O, O, i64);
/// A value with the notes saying how it was obtained.
pub type Noted<T> = (T, Vec<String>);

/// An additive category with graded, finite-dimensional Hom spaces over a field, in which
/// complexes are built.
pub trait Additive {
    /// Coefficients of linear combinations of morphisms (a field for linear algebra).
    type Field: Ring;
    /// An indecomposable object (before shifting).
    type Obj: Clone;
    /// Objects with equal keys are equal (the same idempotent, say).
    type Key: Clone + Ord;
    type Mor: Clone;

    fn field(&self) -> &Self::Field;
    fn parse_obj(&self, s: &str) -> Result<Self::Obj>;
    fn fmt_obj(&self, o: &Self::Obj) -> String;
    fn key(&self, o: &Self::Obj) -> Self::Key;
    fn parse_mor(&self, s: &str) -> Result<Self::Mor>;
    /// Normal form text; `0` for zero.
    fn fmt_mor(&self, f: &Self::Mor) -> Result<String>;
    fn zero(&self) -> Self::Mor;
    /// The identity of an object.
    fn identity(&self, o: &Self::Obj) -> Self::Mor;
    fn add(&self, f: &Self::Mor, g: &Self::Mor) -> Self::Mor;
    fn neg(&self, f: &Self::Mor) -> Self::Mor;
    fn scale(&self, c: &<Self::Field as Ring>::Elem, f: &Self::Mor) -> Self::Mor;
    /// `g ∘ f`.
    fn compose(&self, g: &Self::Mor, f: &Self::Mor) -> Self::Mor;
    fn is_zero(&self, f: &Self::Mor) -> Result<bool>;
    /// Checks that `f` is a morphism `src → tgt` and returns its degree (`None` for zero); an
    /// error says what is wrong (other ends, or not homogeneous).
    fn mor_degree(&self, src: &Self::Obj, tgt: &Self::Obj, f: &Self::Mor) -> Result<Option<i64>>;
    /// The source, target, and degree of a nonzero homogeneous morphism, read off the morphism
    /// itself (`None` for zero); used to infer the objects and shifts a complex leaves out.
    fn mor_ends(&self, f: &Self::Mor) -> Result<Option<MorEnds<Self::Obj>>>;
    /// The least and greatest degree in which `Hom(src, tgt)` can be nonzero (`None`: zero
    /// space; greatest `None`: unbounded above).
    fn hom_degrees(&self, src: &Self::Obj, tgt: &Self::Obj) -> Result<Option<(i64, Option<i64>)>>;
    /// A basis of the degree-`d` part of `Hom(src, tgt)`.
    fn hom_basis(&self, src: &Self::Obj, tgt: &Self::Obj, d: i64) -> Result<Vec<Self::Mor>>;
    /// The coordinates of `f ∈ Hom(src, tgt)_d` in [`Additive::hom_basis`].
    fn coords(
        &self,
        src: &Self::Obj,
        tgt: &Self::Obj,
        d: i64,
        f: &Self::Mor,
    ) -> Result<Vec<<Self::Field as Ring>::Elem>>;
    /// Objects whose Hom functors detect the homology of complexes built from `objs`.
    fn test_objects(&self, objs: &[Self::Obj]) -> Result<Vec<Self::Obj>>;
    /// Whether [`Additive::hom_basis`] and [`Additive::coords`] can be used (a field of
    /// coefficients, numeric scalars); the error says why not.
    fn linear_algebra(&self) -> Result<()>;
    /// How results are obtained: the evaluator's provenance.
    fn provenance(&self) -> Value;
}

/// The direction of the differentials.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Indexing {
    /// `d : C^p → C^{p+1}` (the default).
    Cohomological,
    /// `d : C_p → C_{p−1}`.
    Homological,
}

/// What the differentials of a multicomplex satisfy on each square.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Squares {
    /// `d_b d_a = d_a d_b` (the default; totalization inserts signs).
    Commute,
    /// `d_b d_a = − d_a d_b`.
    Anticommute,
}

/// The sign rule for odd morphisms (super categories): it enters totalization and Hom complexes.
/// In an even category both rules agree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OddSigns {
    /// `(f ⊗ g)(x ⊗ y) = (−1)^{|g||x|} f(x) ⊗ g(y)` (the default).
    Koszul,
    /// No signs from parity.
    Ignore,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Conventions {
    pub indexing: Indexing,
    pub squares: Squares,
    pub odd: OddSigns,
}

impl Conventions {
    /// The step of a differential: `+1` or `−1`.
    pub fn step(&self) -> i64 {
        match self.indexing {
            Indexing::Cohomological => 1,
            Indexing::Homological => -1,
        }
    }

    pub fn parse(v: &Value) -> Result<Self> {
        let s = |k: &str, default: &str| -> Result<String> {
            match v.get(k) {
                None | Some(Value::Null) => Ok(default.into()),
                Some(Value::String(s)) => Ok(s.clone()),
                Some(_) => Err(Error::invalid(format!("'{k}' must be a string"))),
            }
        };
        let indexing = match s("indexing", "cohomological")?.as_str() {
            "cohomological" => Indexing::Cohomological,
            "homological" => Indexing::Homological,
            o => {
                return Err(Error::invalid(format!(
                    "indexing is 'cohomological' or 'homological', not '{o}'"
                )));
            }
        };
        let squares = match s("squares", "commute")?.as_str() {
            "commute" => Squares::Commute,
            "anticommute" => Squares::Anticommute,
            o => {
                return Err(Error::invalid(format!(
                    "squares is 'commute' or 'anticommute', not '{o}'"
                )));
            }
        };
        let odd = match s("odd_signs", "koszul")?.as_str() {
            "koszul" => OddSigns::Koszul,
            "ignore" => OddSigns::Ignore,
            o => {
                return Err(Error::invalid(format!(
                    "odd_signs is 'koszul' or 'ignore', not '{o}'"
                )));
            }
        };
        Ok(Conventions {
            indexing,
            squares,
            odd,
        })
    }

    pub fn to_json(self) -> Value {
        json!({
            "indexing": match self.indexing {
                Indexing::Cohomological => "cohomological",
                Indexing::Homological => "homological",
            },
            "squares": match self.squares {
                Squares::Commute => "commute",
                Squares::Anticommute => "anticommute",
            },
            "odd_signs": match self.odd {
                OddSigns::Koszul => "koszul",
                OddSigns::Ignore => "ignore",
            },
        })
    }
}

/// A position in `ℤⁿ`.
pub type Pos = Vec<i64>;

/// A shifted indecomposable `X⟨s⟩`.
#[derive(Clone, Debug)]
pub struct Summand<O> {
    pub obj: O,
    pub shift: i64,
}

/// A matrix of morphisms: `rows[i][j]` is the entry from source summand `j` to target summand `i`.
pub type Matrix<M> = Vec<Vec<M>>;

/// A bounded complex with finitely many nonzero positions.
pub struct Complex<O, M> {
    pub dim: usize,
    pub conv: Conventions,
    pub terms: BTreeMap<Pos, Vec<Summand<O>>>,
    /// `(axis, from)` ↦ the matrix of the differential from `from` along `axis`.
    pub diffs: BTreeMap<(usize, Pos), Matrix<M>>,
}

/// Formats a position: `3` in one dimension, `(1, 2)` otherwise.
pub fn fmt_pos(p: &[i64]) -> String {
    if p.len() == 1 {
        p[0].to_string()
    } else {
        format!(
            "({})",
            p.iter()
                .map(|x| x.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

fn parse_pos(v: &Value, dim: Option<usize>) -> Result<Pos> {
    let p: Pos = match v {
        Value::Number(n) => vec![
            n.as_i64()
                .ok_or_else(|| Error::invalid("a position is an integer"))?,
        ],
        Value::Array(xs) => xs
            .iter()
            .map(|x| {
                x.as_i64()
                    .ok_or_else(|| Error::invalid("a position is a list of integers"))
            })
            .collect::<Result<_>>()?,
        _ => {
            return Err(Error::invalid(
                "a position is an integer or a list of integers",
            ));
        }
    };
    if p.is_empty() {
        return Err(Error::invalid("a position needs at least one coordinate"));
    }
    if let Some(n) = dim
        && p.len() != n
    {
        return Err(Error::invalid(format!(
            "the position {} has {} coordinates, but the complex has dimension {n}",
            fmt_pos(&p),
            p.len()
        )));
    }
    Ok(p)
}

/// Parses a position given by the caller.
pub fn position(v: &Value, dim: usize) -> Result<Pos> {
    parse_pos(v, Some(dim))
}

fn entry_text(v: &Value) -> Result<Option<String>> {
    match v {
        Value::Null => Ok(None),
        Value::String(s) if s.trim().is_empty() || s.trim() == "0" => Ok(None),
        Value::String(s) => Ok(Some(s.clone())),
        _ => Err(Error::invalid("a matrix entry is a term (string) or null")),
    }
}

/// Parses a matrix of terms with `rows × cols` entries; `where_` locates errors.
pub fn parse_matrix<A: Additive>(
    a: &A,
    v: &Value,
    rows: usize,
    cols: usize,
    where_: &str,
) -> Result<Matrix<A::Mor>> {
    let rs = v
        .as_array()
        .ok_or_else(|| Error::invalid(format!("{where_}: a matrix is a list of rows")))?;
    if rs.len() != rows {
        return Err(Error::invalid(format!(
            "{where_}: the matrix has {} rows, but the target has {rows} summands",
            rs.len()
        )));
    }
    let mut out = Vec::with_capacity(rows);
    for (i, r) in rs.iter().enumerate() {
        let cs = r
            .as_array()
            .ok_or_else(|| Error::invalid(format!("{where_}: row {} is not a list", i + 1)))?;
        if cs.len() != cols {
            return Err(Error::invalid(format!(
                "{where_}: row {} has {} entries, but the source has {cols} summands",
                i + 1,
                cs.len()
            )));
        }
        let mut row = Vec::with_capacity(cols);
        for (j, c) in cs.iter().enumerate() {
            row.push(match entry_text(c)? {
                None => a.zero(),
                Some(t) => a.parse_mor(&t).map_err(|e| {
                    Error::invalid(format!(
                        "{where_}, entry ({}, {}): {}",
                        i + 1,
                        j + 1,
                        e.message
                    ))
                })?,
            });
        }
        out.push(row);
    }
    Ok(out)
}

/// The keys of a complex (`spec/ops/common.schema.json#/$defs/complex`).
pub const COMPLEX_KEYS: [&str; 7] = [
    "dimension",
    "indexing",
    "squares",
    "odd_signs",
    "terms",
    "differentials",
    "resolves",
];

/// The keys of a summand.
const SUMMAND_KEYS: [&str; 3] = ["object", "loading", "shift"];

/// Parses summands `[{"object": X, "shift": s}]` (`loading` is accepted for `object`).
pub fn parse_summands<A: Additive>(a: &A, v: &Value, where_: &str) -> Result<Vec<Summand<A::Obj>>> {
    let xs = v
        .as_array()
        .ok_or_else(|| Error::invalid(format!("{where_}: summands are a list")))?;
    xs.iter()
        .enumerate()
        .map(|(k, x)| {
            crate::keys::check(x, &SUMMAND_KEYS, &format!("{where_}, summand {}", k + 1))?;
            let o = x
                .get("object")
                .or_else(|| x.get("loading"))
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    Error::invalid(format!("{where_}, summand {}: needs 'object'", k + 1))
                })?;
            let shift = match x.get("shift") {
                None | Some(Value::Null) => 0,
                Some(s) => s.as_i64().ok_or_else(|| {
                    Error::invalid(format!(
                        "{where_}, summand {}: 'shift' is an integer",
                        k + 1
                    ))
                })?,
            };
            Ok(Summand {
                obj: a.parse_obj(o).map_err(|e| {
                    Error::invalid(format!("{where_}, summand {}: {}", k + 1, e.message))
                })?,
                shift,
            })
        })
        .collect()
}

impl<O: Clone, M: Clone> Complex<O, M> {
    /// The summands at `p` (none outside the support).
    pub fn at(&self, p: &[i64]) -> &[Summand<O>] {
        self.terms.get(p).map(Vec::as_slice).unwrap_or(&[])
    }

    /// The target of the differential from `p` along `axis`.
    pub fn next(&self, p: &[i64], axis: usize) -> Pos {
        let mut q = p.to_vec();
        q[axis] += self.conv.step();
        q
    }

    /// The source of the differential into `p` along `axis`.
    pub fn prev(&self, p: &[i64], axis: usize) -> Pos {
        let mut q = p.to_vec();
        q[axis] -= self.conv.step();
        q
    }

    /// The positions of the support, together with positions adjacent along an axis.
    pub fn positions(&self) -> BTreeSet<Pos> {
        self.terms.keys().cloned().collect()
    }
}

impl<O: Clone, M: Clone> Complex<O, M> {
    /// The differential from `p` along `axis` (the zero matrix when none is given).
    pub fn diff<A: Additive<Obj = O, Mor = M>>(&self, a: &A, axis: usize, p: &[i64]) -> Matrix<M> {
        match self.diffs.get(&(axis, p.to_vec())) {
            Some(m) => m.clone(),
            None => zero_matrix(a, self.at(&self.next(p, axis)).len(), self.at(p).len()),
        }
    }
}

pub fn zero_matrix<A: Additive>(a: &A, rows: usize, cols: usize) -> Matrix<A::Mor> {
    vec![vec![a.zero(); cols]; rows]
}

/// Parses a complex (`spec/ops/complex.check.schema.json`, `$defs/complex`).
pub fn parse_complex<A: Additive>(a: &A, v: &Value) -> Result<Complex<A::Obj, A::Mor>> {
    Ok(parse_complex_noted(a, v)?.0)
}

/// A summand as written: its object and shift may be left out, to be inferred.
type Written<O> = (Option<O>, Option<i64>);

/// [`parse_complex`], with notes saying which objects and shifts were inferred. A summand may
/// leave out its `object` or its `shift`: each nonzero entry `f : X⟨s⟩ → Y⟨t⟩` of a differential
/// fixes the objects at its ends and the difference `s − t = deg f`. Shifts are propagated along
/// the entries; a group of summands that no written shift reaches gets shift 0 at its first
/// summand, taking the positions from the end of the differentials (the last position for
/// cohomological indexing, the first for homological) and the summands in order. Inferred
/// values that contradict each other are left for [`check_complex`] to report.
pub fn parse_complex_noted<A: Additive>(
    a: &A,
    v: &Value,
) -> Result<Noted<Complex<A::Obj, A::Mor>>> {
    crate::keys::check(v, &COMPLEX_KEYS, "a complex")?;
    let conv = Conventions::parse(v)?;
    let terms_v = v
        .get("terms")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::invalid("a complex needs 'terms', a list of {at, summands}"))?;
    let mut dim = match v.get("dimension") {
        None | Some(Value::Null) => None,
        Some(d) => Some(
            d.as_u64()
                .filter(|&d| d >= 1)
                .ok_or_else(|| Error::invalid("'dimension' is a positive integer"))?
                as usize,
        ),
    };
    let mut written: BTreeMap<Pos, Vec<Written<A::Obj>>> = BTreeMap::new();
    for t in terms_v {
        crate::keys::check(t, &["at", "summands"], "a term")?;
        let p = parse_pos(
            t.get("at")
                .ok_or_else(|| Error::invalid("a term needs 'at', its position"))?,
            dim,
        )?;
        dim.get_or_insert(p.len());
        let where_ = format!("the term at {}", fmt_pos(&p));
        let ss = parse_written(
            a,
            t.get("summands")
                .ok_or_else(|| Error::invalid(format!("{where_} needs 'summands'")))?,
            &where_,
        )?;
        if written.insert(p.clone(), ss).is_some() {
            return Err(Error::invalid(format!(
                "two terms are at the position {}",
                fmt_pos(&p)
            )));
        }
    }
    let dim = dim.unwrap_or(1);
    let step = conv.step();
    let next = |p: &[i64], axis: usize| {
        let mut q = p.to_vec();
        q[axis] += step;
        q
    };
    let len = |p: &Pos| written.get(p).map_or(0, Vec::len);
    let mut diffs = BTreeMap::new();
    // written entries that are zero in this context: (where, row, column, text)
    let mut vanished: Vec<(String, usize, usize, String)> = Vec::new();
    if let Some(ds) = v.get("differentials") {
        let ds = ds
            .as_array()
            .ok_or_else(|| Error::invalid("'differentials' is a list"))?;
        for d in ds {
            crate::keys::check(d, &["from", "axis", "matrix"], "a differential")?;
            let axis = match d.get("axis") {
                None | Some(Value::Null) => 0,
                Some(x) => x.as_u64().ok_or_else(|| {
                    Error::invalid("'axis' is a nonnegative integer (axes are numbered from 0)")
                })? as usize,
            };
            if axis >= dim {
                return Err(Error::invalid(format!(
                    "axis {axis} does not exist: the complex has dimension {dim} (axes are \
                     numbered from 0)"
                )));
            }
            let p = parse_pos(
                d.get("from")
                    .ok_or_else(|| Error::invalid("a differential needs 'from', its source"))?,
                Some(dim),
            )?;
            let q = next(&p, axis);
            let where_ = if dim == 1 {
                format!("the differential from {}", fmt_pos(&p))
            } else {
                format!("the differential from {} along axis {axis}", fmt_pos(&p))
            };
            let m = parse_matrix(
                a,
                d.get("matrix")
                    .ok_or_else(|| Error::invalid(format!("{where_} needs 'matrix'")))?,
                len(&q),
                len(&p),
                &where_,
            )?;
            for (r, row) in m.iter().enumerate() {
                for (c, f) in row.iter().enumerate() {
                    if let Some(t) = entry_text(&d["matrix"][r][c])?
                        && a.is_zero(f)?
                    {
                        vanished.push((where_.clone(), r + 1, c + 1, t.trim().to_string()));
                    }
                }
            }
            if diffs.insert((axis, p.clone()), m).is_some() {
                return Err(Error::invalid(format!("{where_} is given twice")));
            }
        }
    }
    // the ends of the nonzero entries: (source summand, target summand, source, target, degree)
    type Node = (Pos, usize);
    type Edge<O> = (Node, Node, O, O, i64);
    let mut edges: Vec<Edge<A::Obj>> = Vec::new();
    for ((axis, p), m) in &diffs {
        let q = next(p, *axis);
        for (r, row) in m.iter().enumerate() {
            for (c, f) in row.iter().enumerate() {
                // an entry that is not a homogeneous map is left for the check to report
                if let Ok(Some((s, t, d))) = a.mor_ends(f) {
                    edges.push(((p.clone(), c), (q.clone(), r), s, t, d));
                }
            }
        }
    }
    let mut notes: Vec<String> = vanished
        .iter()
        .map(|(w, r, c, t)| {
            format!(
                "{w}, entry ({r}, {c}): '{t}' is zero in this context, so the entry is zero \
                 (shown as null)"
            )
        })
        .collect();
    let (mut objs_inferred, mut shifts_inferred) = (0usize, 0usize);
    for (from, to, s, t, _) in &edges {
        for (node, o) in [(from, s), (to, t)] {
            let slot = &mut written.get_mut(&node.0).unwrap()[node.1].0;
            if slot.is_none() {
                *slot = Some(o.clone());
                objs_inferred += 1;
            }
        }
    }
    // the order in which unreached groups are anchored: from the end of the differentials
    let mut order: Vec<Node> = written
        .iter()
        .flat_map(|(p, ss)| (0..ss.len()).map(move |k| (p.clone(), k)))
        .collect();
    order.sort_by_key(|(p, k)| (-step * p.iter().sum::<i64>(), p.clone(), *k));
    let mut anchors = Vec::new();
    loop {
        let mut progress = true;
        while progress {
            progress = false;
            for (from, to, _, _, d) in &edges {
                let sf = written[&from.0][from.1].1;
                let st = written[&to.0][to.1].1;
                match (sf, st) {
                    (Some(x), None) => {
                        written.get_mut(&to.0).unwrap()[to.1].1 = Some(x - d);
                        shifts_inferred += 1;
                        progress = true;
                    }
                    (None, Some(y)) => {
                        written.get_mut(&from.0).unwrap()[from.1].1 = Some(y + d);
                        shifts_inferred += 1;
                        progress = true;
                    }
                    _ => {}
                }
            }
        }
        match order.iter().find(|(p, k)| written[p][*k].1.is_none()) {
            None => break,
            Some((p, k)) => {
                let connected = edges
                    .iter()
                    .any(|(f, t, ..)| (&f.0 == p && f.1 == *k) || (&t.0 == p && t.1 == *k));
                written.get_mut(p).unwrap()[*k].1 = Some(0);
                if connected {
                    anchors.push((p.clone(), *k));
                }
            }
        }
    }
    let mut terms = BTreeMap::new();
    for (p, ss) in written {
        let mut out = Vec::with_capacity(ss.len());
        for (k, (o, s)) in ss.into_iter().enumerate() {
            let obj = o.ok_or_else(|| {
                Error::invalid(format!(
                    "the term at {}, summand {}: needs 'object' (no nonzero entry of a \
                     differential starts or ends there, so it cannot be inferred)",
                    fmt_pos(&p),
                    k + 1
                ))
            })?;
            out.push(Summand {
                obj,
                shift: s.unwrap_or(0),
            });
        }
        terms.insert(p, out);
    }
    if objs_inferred > 0 {
        notes.push(format!(
            "the objects of {objs_inferred} summand{} were read off the differential entries",
            if objs_inferred == 1 { "" } else { "s" }
        ));
    }
    if shifts_inferred > 0 || !anchors.is_empty() {
        let anchored: Vec<String> = anchors
            .iter()
            .map(|(p, k)| format!("{}⟨0⟩ at {}", a.fmt_obj(&terms[p][*k].obj), fmt_pos(p)))
            .collect();
        notes.push(format!(
            "the shifts of {} summand{} were inferred from the degrees of the differential \
             entries{}",
            shifts_inferred + anchors.len(),
            if shifts_inferred + anchors.len() == 1 {
                ""
            } else {
                "s"
            },
            if anchored.is_empty() {
                String::new()
            } else {
                format!(", starting from {}", anchored.join(", "))
            }
        ));
    }
    Ok((
        Complex {
            dim,
            conv,
            terms,
            diffs,
        },
        notes,
    ))
}

/// The summands of a term as written, each with an optional object and shift.
fn parse_written<A: Additive>(a: &A, v: &Value, where_: &str) -> Result<Vec<Written<A::Obj>>> {
    let xs = v
        .as_array()
        .ok_or_else(|| Error::invalid(format!("{where_}: summands are a list")))?;
    xs.iter()
        .enumerate()
        .map(|(k, x)| {
            crate::keys::check(x, &SUMMAND_KEYS, &format!("{where_}, summand {}", k + 1))?;
            let o = match x.get("object").or_else(|| x.get("loading")) {
                None | Some(Value::Null) => None,
                Some(Value::String(o)) => Some(a.parse_obj(o).map_err(|e| {
                    Error::invalid(format!("{where_}, summand {}: {}", k + 1, e.message))
                })?),
                Some(_) => {
                    return Err(Error::invalid(format!(
                        "{where_}, summand {}: 'object' is a string such as \"e(i|1)\"",
                        k + 1
                    )));
                }
            };
            let shift = match x.get("shift") {
                None | Some(Value::Null) => None,
                Some(s) => Some(s.as_i64().ok_or_else(|| {
                    Error::invalid(format!(
                        "{where_}, summand {}: 'shift' is an integer",
                        k + 1
                    ))
                })?),
            };
            Ok((o, shift))
        })
        .collect()
}

/// The JSON form of a complex, with entries in normal form (`null` for zero).
pub fn complex_json<A: Additive>(a: &A, c: &Complex<A::Obj, A::Mor>) -> Result<Value> {
    let mut v = c.conv.to_json();
    v["dimension"] = json!(c.dim);
    v["terms"] = Value::Array(
        c.terms
            .iter()
            .map(|(p, ss)| {
                json!({
                    "at": pos_json(p),
                    "summands": summands_json(a, ss),
                })
            })
            .collect(),
    );
    let mut ds = Vec::new();
    for ((axis, p), m) in &c.diffs {
        let mut d = json!({ "from": pos_json(p), "matrix": matrix_json(a, m)? });
        if c.dim > 1 {
            d["axis"] = json!(axis);
        }
        ds.push(d);
    }
    v["differentials"] = Value::Array(ds);
    Ok(v)
}

pub fn pos_json(p: &[i64]) -> Value {
    if p.len() == 1 { json!(p[0]) } else { json!(p) }
}

pub fn summands_json<A: Additive>(a: &A, ss: &[Summand<A::Obj>]) -> Value {
    Value::Array(
        ss.iter()
            .map(|s| json!({ "object": a.fmt_obj(&s.obj), "shift": s.shift }))
            .collect(),
    )
}

pub fn matrix_json<A: Additive>(a: &A, m: &Matrix<A::Mor>) -> Result<Value> {
    let mut rows = Vec::new();
    for r in m {
        let mut row = Vec::new();
        for f in r {
            row.push(if a.is_zero(f)? {
                Value::Null
            } else {
                json!(a.fmt_mor(f)?)
            });
        }
        rows.push(Value::Array(row));
    }
    Ok(Value::Array(rows))
}

/// `g · f` (first `f`, then `g`), with `cols` columns. The inner size is the number of rows of
/// `f`; `cols` is explicit, since a matrix with no rows does not record its number of columns.
pub fn mat_mul<A: Additive>(
    a: &A,
    g: &Matrix<A::Mor>,
    f: &Matrix<A::Mor>,
    cols: usize,
) -> Matrix<A::Mor> {
    // the inner dimension is the number of rows of `f`, never passed by the caller: a wrong
    // inner size silently drops summands of the middle object
    let inner = f.len();
    g.iter()
        .map(|grow| {
            assert_eq!(
                grow.len(),
                inner,
                "mat_mul: g has {} columns but f has {inner} rows",
                grow.len()
            );
            (0..cols)
                .map(|j| {
                    let mut acc = a.zero();
                    for (gk, frow) in grow.iter().zip(f) {
                        acc = a.add(&acc, &a.compose(gk, &frow[j]));
                    }
                    acc
                })
                .collect()
        })
        .collect()
}

/// `f + sign · g`.
pub fn mat_add<A: Additive>(
    a: &A,
    f: &Matrix<A::Mor>,
    g: &Matrix<A::Mor>,
    sign: i64,
) -> Matrix<A::Mor> {
    f.iter()
        .zip(g)
        .map(|(r, s)| {
            r.iter()
                .zip(s)
                .map(|(x, y)| {
                    if sign > 0 {
                        a.add(x, y)
                    } else {
                        a.add(x, &a.neg(y))
                    }
                })
                .collect()
        })
        .collect()
}

/// A failed condition, located.
#[derive(Clone, Debug)]
pub struct Failure {
    pub condition: &'static str,
    pub location: String,
    pub detail: String,
    /// The maps the failure is about, so a view can mark them: each is `{"map": "differential",
    /// "from": p, "axis": a}`, `{"map": "component", "at": p}`, `{"map": "augmentation"}`, or
    /// `{"map": "presentation"}`, with `row` and `col` when one entry is meant.
    pub maps: Vec<Value>,
}

impl Failure {
    pub fn json(&self) -> Value {
        json!({
            "condition": self.condition,
            "location": self.location,
            "detail": self.detail,
            "maps": self.maps,
        })
    }
}

/// The reference `{"map": "differential", "from": p, "axis": a}` to a differential of a complex.
pub fn diff_ref(p: &[i64], axis: usize) -> Value {
    json!({ "map": "differential", "from": pos_json(p), "axis": axis })
}

/// The reference `{"map": "component", "at": p}` to a component of a map of complexes.
pub fn component_ref(p: &[i64]) -> Value {
    json!({ "map": "component", "at": pos_json(p) })
}

fn entry_ref(map: &Value, i: usize, j: usize) -> Vec<Value> {
    if map.is_null() {
        return vec![];
    }
    let mut r = map.clone();
    r["row"] = json!(i);
    r["col"] = json!(j);
    vec![r]
}

/// Checks the entries of a matrix from `src` to `tgt`: each lies in the right Hom space and has
/// the degree of a degree-zero map between the shifted summands (plus `extra`).
#[allow(clippy::too_many_arguments)]
pub fn check_entries<A: Additive>(
    a: &A,
    src: &[Summand<A::Obj>],
    tgt: &[Summand<A::Obj>],
    m: &Matrix<A::Mor>,
    extra: i64,
    location: &str,
    map: &Value,
    out: &mut Vec<Failure>,
) {
    for (i, row) in m.iter().enumerate() {
        for (j, f) in row.iter().enumerate() {
            let (s, t) = (&src[j], &tgt[i]);
            let loc = format!("{location}, entry ({}, {})", i + 1, j + 1);
            match a.mor_degree(&s.obj, &t.obj, f) {
                Err(e) => out.push(Failure {
                    condition: "entry",
                    location: loc,
                    detail: e.message,
                    maps: entry_ref(map, i, j),
                }),
                Ok(None) => {}
                Ok(Some(d)) => {
                    let want = s.shift - t.shift + extra;
                    if d != want {
                        out.push(Failure {
                            condition: "degree",
                            location: loc,
                            detail: format!(
                                "the entry has degree {d}; a degree-zero map from {}⟨{}⟩ to \
                                 {}⟨{}⟩ needs degree {want}",
                                a.fmt_obj(&s.obj),
                                s.shift,
                                a.fmt_obj(&t.obj),
                                t.shift
                            ),
                            maps: entry_ref(map, i, j),
                        });
                    }
                }
            }
        }
    }
}

/// Records every nonzero entry of `m` as a failure of `condition`.
pub fn check_zero<A: Additive>(
    a: &A,
    m: &Matrix<A::Mor>,
    condition: &'static str,
    location: &str,
    maps: &[Value],
    out: &mut Vec<Failure>,
) -> Result<()> {
    for (i, row) in m.iter().enumerate() {
        for (j, f) in row.iter().enumerate() {
            if !a.is_zero(f)? {
                out.push(Failure {
                    condition,
                    location: format!("{location}, entry ({}, {})", i + 1, j + 1),
                    detail: a.fmt_mor(f)?,
                    maps: maps.to_vec(),
                });
            }
        }
    }
    Ok(())
}

fn axis_name<O, M>(c: &Complex<O, M>, axis: usize) -> String {
    if c.dim == 1 {
        String::new()
    } else {
        format!(" along axis {axis}")
    }
}

/// Checks a complex: entries in the right Hom spaces and of the right degrees, `d ∘ d = 0` along
/// each axis, and the square condition of the conventions.
pub fn check_complex<A: Additive>(a: &A, c: &Complex<A::Obj, A::Mor>) -> Result<Vec<Failure>> {
    let mut out = Vec::new();
    for ((axis, p), m) in &c.diffs {
        let loc = format!(
            "the differential from {}{}",
            fmt_pos(p),
            axis_name(c, *axis)
        );
        let r = diff_ref(p, *axis);
        check_entries(
            a,
            c.at(p),
            c.at(&c.next(p, *axis)),
            m,
            0,
            &loc,
            &r,
            &mut out,
        );
    }
    if !out.is_empty() {
        return Ok(out);
    }
    for p in c.positions() {
        for axis in 0..c.dim {
            let q = c.next(&p, axis);
            if c.at(&q).is_empty() {
                continue;
            }
            let d1 = c.diff(a, axis, &p);
            let d2 = c.diff(a, axis, &q);
            let dd = mat_mul(a, &d2, &d1, c.at(&p).len());
            let loc = format!("d ∘ d from {}{}", fmt_pos(&p), axis_name(c, axis));
            let maps = [diff_ref(&p, axis), diff_ref(&q, axis)];
            check_zero(a, &dd, "d^2", &loc, &maps, &mut out)?;
            for b in axis + 1..c.dim {
                let pb = c.next(&p, b);
                let pab = c.next(&q, b);
                if c.at(&pab).is_empty() {
                    continue;
                }
                // d_b(p + e_a) d_a(p) ∓ d_a(p + e_b) d_b(p)
                let x = mat_mul(a, &c.diff(a, b, &q), &d1, c.at(&p).len());
                let y = mat_mul(a, &c.diff(a, axis, &pb), &c.diff(a, b, &p), c.at(&p).len());
                let sign = match c.conv.squares {
                    Squares::Commute => -1,
                    Squares::Anticommute => 1,
                };
                let s = mat_add(a, &x, &y, sign);
                let loc = format!("the square at {} on axes {axis} and {b}", fmt_pos(&p));
                let maps = [
                    diff_ref(&p, axis),
                    diff_ref(&q, b),
                    diff_ref(&p, b),
                    diff_ref(&pb, axis),
                ];
                check_zero(a, &s, "square", &loc, &maps, &mut out)?;
            }
        }
    }
    Ok(out)
}

/// Maps of complexes `F : C → D`: `F(p) : C(p) → D(p)` for each position.
pub type Map<M> = BTreeMap<Pos, Matrix<M>>;

/// Parses `[{"at": p, "matrix": ...}]` with matrices from `src(p)` to `tgt(p + offset)`.
pub fn parse_map<A: Additive>(
    a: &A,
    v: &Value,
    src: &Complex<A::Obj, A::Mor>,
    tgt: &Complex<A::Obj, A::Mor>,
    offset: &[i64],
    name: &str,
) -> Result<Map<A::Mor>> {
    let xs = v
        .as_array()
        .ok_or_else(|| Error::invalid(format!("'{name}' is a list of {{at, matrix}}")))?;
    let mut out = Map::new();
    for x in xs {
        crate::keys::check(x, &["at", "matrix"], &format!("a component of '{name}'"))?;
        let p = parse_pos(
            x.get("at")
                .ok_or_else(|| Error::invalid(format!("each component of '{name}' needs 'at'")))?,
            Some(src.dim),
        )?;
        let q: Pos = p.iter().zip(offset).map(|(x, o)| x + o).collect();
        let where_ = format!("the component of '{name}' at {}", fmt_pos(&p));
        let m = parse_matrix(
            a,
            x.get("matrix")
                .ok_or_else(|| Error::invalid(format!("{where_} needs 'matrix'")))?,
            tgt.at(&q).len(),
            src.at(&p).len(),
            &where_,
        )?;
        if out.insert(p, m).is_some() {
            return Err(Error::invalid(format!("{where_} is given twice")));
        }
    }
    Ok(out)
}

fn component<A: Additive>(
    a: &A,
    f: &Map<A::Mor>,
    p: &[i64],
    rows: usize,
    cols: usize,
) -> Matrix<A::Mor> {
    f.get(p)
        .cloned()
        .unwrap_or_else(|| zero_matrix(a, rows, cols))
}

fn same_shape<O, M>(c: &Complex<O, M>, d: &Complex<O, M>) -> Result<()> {
    if c.dim != d.dim || c.conv.indexing != d.conv.indexing {
        return Err(Error::invalid(
            "the two complexes need the same dimension and indexing",
        ));
    }
    Ok(())
}

/// Checks a chain map: entries of degree zero, and `d_D F = F d_C` along every axis.
pub fn check_chain_map<A: Additive>(
    a: &A,
    c: &Complex<A::Obj, A::Mor>,
    d: &Complex<A::Obj, A::Mor>,
    f: &Map<A::Mor>,
) -> Result<Vec<Failure>> {
    same_shape(c, d)?;
    let mut out = Vec::new();
    for (p, m) in f {
        let loc = format!("the map at {}", fmt_pos(p));
        check_entries(a, c.at(p), d.at(p), m, 0, &loc, &component_ref(p), &mut out);
    }
    if !out.is_empty() {
        return Ok(out);
    }
    let mut ps: BTreeSet<Pos> = c.positions();
    ps.extend(d.positions());
    for p in ps {
        for axis in 0..c.dim {
            let q = c.next(&p, axis);
            if c.at(&p).is_empty() && d.at(&q).is_empty() {
                continue;
            }
            let fp = component(a, f, &p, d.at(&p).len(), c.at(&p).len());
            let fq = component(a, f, &q, d.at(&q).len(), c.at(&q).len());
            let x = mat_mul(a, &d.diff(a, axis, &p), &fp, c.at(&p).len());
            let y = mat_mul(a, &fq, &c.diff(a, axis, &p), c.at(&p).len());
            let s = mat_add(a, &x, &y, -1);
            let loc = format!("d F − F d from {}{}", fmt_pos(&p), axis_name(c, axis));
            check_zero(
                a,
                &s,
                "chain map",
                &loc,
                &[component_ref(&p), component_ref(&q)],
                &mut out,
            )?;
        }
    }
    Ok(out)
}

/// Checks a homotopy `f − g = d h + h d` of maps of one-dimensional complexes, where `h(p) :
/// C(p) → D(p − step)`.
pub fn check_homotopy<A: Additive>(
    a: &A,
    c: &Complex<A::Obj, A::Mor>,
    d: &Complex<A::Obj, A::Mor>,
    f: &Map<A::Mor>,
    g: &Map<A::Mor>,
    h: &Map<A::Mor>,
) -> Result<Vec<Failure>> {
    same_shape(c, d)?;
    if c.dim != 1 {
        return Err(Error::precondition(
            "homotopies are checked for one-dimensional complexes; totalize first",
        ));
    }
    let mut out = Vec::new();
    for (p, m) in h {
        let loc = format!("the homotopy at {}", fmt_pos(p));
        check_entries(
            a,
            c.at(p),
            d.at(&c.prev(p, 0)),
            m,
            0,
            &loc,
            &Value::Null,
            &mut out,
        );
    }
    for (name, map) in [("f", f), ("g", g)] {
        for (p, m) in map {
            let loc = format!("the map {name} at {}", fmt_pos(p));
            check_entries(a, c.at(p), d.at(p), m, 0, &loc, &Value::Null, &mut out);
        }
    }
    if !out.is_empty() {
        return Ok(out);
    }
    let mut ps: BTreeSet<Pos> = c.positions();
    ps.extend(d.positions());
    for p in ps {
        let (rows, cols) = (d.at(&p).len(), c.at(&p).len());
        if rows == 0 || cols == 0 {
            continue;
        }
        let back = c.prev(&p, 0);
        let fwd = c.next(&p, 0);
        let fg = mat_add(
            a,
            &component(a, f, &p, rows, cols),
            &component(a, g, &p, rows, cols),
            -1,
        );
        // d_D(back) h(p) : C(p) → D(back) → D(p)
        let hp = component(a, h, &p, d.at(&back).len(), cols);
        let x = mat_mul(a, &d.diff(a, 0, &back), &hp, cols);
        // h(fwd) d_C(p) : C(p) → C(fwd) → D(p)
        let hf = component(a, h, &fwd, rows, c.at(&fwd).len());
        let y = mat_mul(a, &hf, &c.diff(a, 0, &p), cols);
        let s = mat_add(a, &mat_add(a, &fg, &x, -1), &y, -1);
        let loc = format!("f − g − (d h + h d) at {}", fmt_pos(&p));
        check_zero(a, &s, "homotopy", &loc, &[], &mut out)?;
    }
    Ok(out)
}

/// The degrees in which `Hom(T, ⊕ X_i⟨s_i⟩)` can be nonzero: the least, and the greatest
/// (`None`: unbounded above). `None` when the space is zero.
pub fn degree_range<A: Additive>(
    a: &A,
    t: &A::Obj,
    ss: &[Summand<A::Obj>],
) -> Result<Option<(i64, Option<i64>)>> {
    let mut out: Option<(i64, Option<i64>)> = None;
    for s in ss {
        if let Some((lo, hi)) = a.hom_degrees(t, &s.obj)? {
            let (lo, hi) = (lo + s.shift, hi.map(|h| h + s.shift));
            out = Some(match out {
                None => (lo, hi),
                Some((l, h)) => (
                    l.min(lo),
                    match (h, hi) {
                        (Some(x), Some(y)) => Some(x.max(y)),
                        _ => None,
                    },
                ),
            });
        }
    }
    Ok(out)
}

/// The degrees in which `Hom(T, ⊕ X_i⟨s_i⟩)` can be nonzero for some test object `T`.
pub fn range_over_tests<A: Additive>(
    a: &A,
    tests: &[A::Obj],
    ss: &[Summand<A::Obj>],
) -> Result<Option<(i64, Option<i64>)>> {
    let mut range: Option<(i64, Option<i64>)> = None;
    for t in tests {
        if let Some((lo, hi)) = degree_range(a, t, ss)? {
            range = Some(match range {
                None => (lo, hi),
                Some((l, h)) => (
                    l.min(lo),
                    match (h, hi) {
                        (Some(x), Some(y)) => Some(x.max(y)),
                        _ => None,
                    },
                ),
            });
        }
    }
    Ok(range)
}

/// A basis of `Hom(T⟨d⟩, ⊕ X_i⟨s_i⟩)` (degree-zero maps): pairs (summand, basis morphism), in
/// order.
pub struct Space<M> {
    pub basis: Vec<(usize, M)>,
    /// `offsets[i]`: index of the first basis vector of summand `i`; `dims[i]` its dimension.
    pub offsets: Vec<usize>,
    pub dims: Vec<usize>,
}

pub fn space<A: Additive>(
    a: &A,
    t: &A::Obj,
    d: i64,
    ss: &[Summand<A::Obj>],
) -> Result<Space<A::Mor>> {
    let mut basis = Vec::new();
    let mut offsets = Vec::new();
    let mut dims = Vec::new();
    for (i, s) in ss.iter().enumerate() {
        offsets.push(basis.len());
        let b = a.hom_basis(t, &s.obj, d - s.shift)?;
        dims.push(b.len());
        basis.extend(b.into_iter().map(|f| (i, f)));
    }
    Ok(Space {
        basis,
        offsets,
        dims,
    })
}

/// The coordinates of a column `(f_i : T → X_i)_i` of degree `d − s_i` in [`space`].
pub fn column_coords<A: Additive>(
    a: &A,
    t: &A::Obj,
    d: i64,
    ss: &[Summand<A::Obj>],
    sp: &Space<A::Mor>,
    col: &[A::Mor],
) -> Result<Vec<<A::Field as Ring>::Elem>> {
    let k = a.field();
    let mut v = vec![k.zero(); sp.basis.len()];
    for (i, f) in col.iter().enumerate() {
        if sp.dims[i] == 0 {
            if !a.is_zero(f)? {
                return Err(Error::invalid(
                    "internal: a nonzero morphism in a zero Hom space",
                ));
            }
            continue;
        }
        let c = a.coords(t, &ss[i].obj, d - ss[i].shift, f)?;
        for (j, x) in c.into_iter().enumerate() {
            v[sp.offsets[i] + j] = x;
        }
    }
    Ok(v)
}

/// The matrix of postcomposition with `m : ⊕ X_j⟨s_j⟩ → ⊕ Y_i⟨t_i⟩` from `Hom(T⟨d⟩, X)` to
/// `Hom(T⟨d⟩, Y)` (rows: target coordinates).
pub fn postcompose<A: Additive>(
    a: &A,
    t: &A::Obj,
    d: i64,
    tgt: &[Summand<A::Obj>],
    m: &Matrix<A::Mor>,
    sp_src: &Space<A::Mor>,
    sp_tgt: &Space<A::Mor>,
) -> Result<Mat<<A::Field as Ring>::Elem>> {
    let mut cols = Vec::with_capacity(sp_src.basis.len());
    for (j, b) in &sp_src.basis {
        let col: Vec<A::Mor> = m.iter().map(|row| a.compose(&row[*j], b)).collect();
        cols.push(column_coords(a, t, d, tgt, sp_tgt, &col)?);
    }
    let rows = sp_tgt.basis.len();
    let n = cols.len();
    Ok(Mat::from_fn(rows, n, |i, j| cols[j][i].clone()))
}

/// The column of morphisms `T⟨d⟩ → ⊕ X_i⟨s_i⟩` with coordinates `v`.
pub fn column_of<A: Additive>(
    a: &A,
    sp: &Space<A::Mor>,
    n: usize,
    v: &[<A::Field as Ring>::Elem],
) -> Vec<A::Mor> {
    let k = a.field();
    let mut col = vec![a.zero(); n];
    for (idx, (i, b)) in sp.basis.iter().enumerate() {
        if !k.is_zero(&v[idx]) {
            col[*i] = a.add(&col[*i], &a.scale(&v[idx], b));
        }
    }
    col
}

/// Kernel generators and the degree window they were searched in.
pub type KernelGenerators<A> = (
    Vec<Generator<<A as Additive>::Obj, <A as Additive>::Mor>>,
    Option<Window>,
);

/// The degrees to compute in: every degree when the spaces are bounded, else up to `max`.
#[derive(Clone, Copy, Debug)]
pub struct Window {
    pub lo: i64,
    pub hi: i64,
    /// Whether some space is nonzero above `hi`.
    pub truncated: bool,
}

pub fn window(
    range: Option<(i64, Option<i64>)>,
    max: Option<i64>,
    default_span: i64,
) -> Option<Window> {
    let (lo, hi) = range?;
    Some(match (hi, max) {
        // a cap below the lowest degree would search nothing: the lowest degree is kept
        (Some(h), Some(m)) => Window {
            lo,
            hi: h.min(m).max(lo),
            truncated: m.max(lo) < h,
        },
        (Some(h), None) => Window {
            lo,
            hi: h,
            truncated: false,
        },
        (None, Some(m)) => Window {
            lo,
            hi: m.max(lo),
            truncated: true,
        },
        (None, None) => Window {
            lo,
            hi: lo + default_span,
            truncated: true,
        },
    })
}

fn rank<K: Ring>(k: &K, m: &Mat<K::Elem>) -> Result<usize> {
    if m.nrows() == 0 || m.ncols() == 0 {
        return Ok(0);
    }
    crate::linalg::rank(k, m)
}

/// The homology at a position along an axis, evaluated at a test object in one degree.
pub struct HomologyPiece<M> {
    pub dim: usize,
    /// Cycles representing a basis of the homology (at most the number asked for).
    pub representatives: Vec<Vec<M>>,
}

/// `H(Hom(T⟨d⟩, C))` at `p` along `axis`: cycles modulo boundaries.
pub fn homology_piece<A: Additive>(
    a: &A,
    c: &Complex<A::Obj, A::Mor>,
    p: &[i64],
    axis: usize,
    t: &A::Obj,
    d: i64,
    reps: usize,
) -> Result<HomologyPiece<A::Mor>> {
    let k = a.field().clone();
    let here = c.at(p);
    let (pp, pn) = (c.prev(p, axis), c.next(p, axis));
    let sp = space(a, t, d, here)?;
    if sp.basis.is_empty() {
        return Ok(HomologyPiece {
            dim: 0,
            representatives: vec![],
        });
    }
    let sp_n = space(a, t, d, c.at(&pn))?;
    let sp_p = space(a, t, d, c.at(&pp))?;
    let out = postcompose(a, t, d, c.at(&pn), &c.diff(a, axis, p), &sp, &sp_n)?;
    let inc = postcompose(a, t, d, here, &c.diff(a, axis, &pp), &sp_p, &sp)?;
    let n = sp.basis.len();
    let r_out = rank(&k, &out)?;
    let r_in = rank(&k, &inc)?;
    let dim = n - r_out - r_in;
    let mut representatives = Vec::new();
    if dim > 0 && reps > 0 {
        let cycles = if out.nrows() == 0 {
            (0..n)
                .map(|i| {
                    let mut v = vec![k.zero(); n];
                    v[i] = k.one();
                    v
                })
                .collect()
        } else {
            crate::linalg::kernel(&k, &out)?
        };
        let mut span = Reducer::new(k.clone(), n);
        for j in 0..inc.ncols() {
            span.insert((0..n).map(|i| inc.get(i, j).clone()).collect());
        }
        for z in cycles {
            if representatives.len() >= reps {
                break;
            }
            if span.insert(z.clone()) {
                representatives.push(column_of(a, &sp, here.len(), &z));
            }
        }
    }
    Ok(HomologyPiece {
        dim,
        representatives,
    })
}

/// A generator of a submodule of `⊕ X_i⟨s_i⟩`: a column of morphisms `T⟨d⟩ → X`.
pub struct Generator<O, M> {
    pub obj: O,
    pub degree: i64,
    pub column: Vec<M>,
}

/// Generators of the kernel of `m : ⊕ X_j⟨s_j⟩ → ⊕ Y_i⟨t_i⟩`, found degree by degree at the
/// test objects: in each degree, the kernel modulo what earlier generators produce. Returns the
/// generators and the degree window searched.
pub fn kernel_generators<A: Additive>(
    a: &A,
    src: &[Summand<A::Obj>],
    tgt: &[Summand<A::Obj>],
    m: &Matrix<A::Mor>,
    tests: &[A::Obj],
    max: Option<i64>,
    default_span: i64,
) -> Result<KernelGenerators<A>> {
    relative_kernel_generators(a, src, tgt, m, None, tests, max, default_span)
}

/// The kernel of `Hom(T⟨d⟩, X) → Hom(T⟨d⟩, Y) / im Hom(T⟨d⟩, φ)`, for `m : X → Y` and
/// `rel = (R, φ : R → Y)` (or the plain kernel of `m` when `rel` is `None`), as vectors in
/// [`space`] of `X`.
#[allow(clippy::too_many_arguments)]
/// Optional relations `(Q₁, φ: Q₁ → tgt)`: a map into `tgt` counts as zero when it factors
/// through `φ`.
pub type Relations<'a, A> = Option<(
    &'a [Summand<<A as Additive>::Obj>],
    &'a Matrix<<A as Additive>::Mor>,
)>;

pub fn relative_kernel_piece<A: Additive>(
    a: &A,
    t: &A::Obj,
    d: i64,
    sp: &Space<A::Mor>,
    tgt: &[Summand<A::Obj>],
    m: &Matrix<A::Mor>,
    rel: Relations<'_, A>,
) -> Result<Vec<Vec<<A::Field as Ring>::Elem>>> {
    let k = a.field().clone();
    let n = sp.basis.len();
    let unit = |n: usize| -> Vec<Vec<_>> {
        (0..n)
            .map(|i| {
                let mut v = vec![k.zero(); n];
                v[i] = k.one();
                v
            })
            .collect()
    };
    let sp_t = space(a, t, d, tgt)?;
    if sp_t.basis.is_empty() {
        return Ok(unit(n));
    }
    let mat = postcompose(a, t, d, tgt, m, sp, &sp_t)?;
    let Some((r, phi)) = rel else {
        return crate::linalg::kernel(&k, &mat);
    };
    let sp_r = space(a, t, d, r)?;
    if sp_r.basis.is_empty() {
        return crate::linalg::kernel(&k, &mat);
    }
    // kernel of [m | −φ]: pairs with m x = φ y, so x carries the natural sign
    let b = postcompose(a, t, d, tgt, phi, &sp_r, &sp_t)?.map(|e| k.neg(e));
    let ab = mat.hstack(&b)?;
    let mut red = Reducer::new(k.clone(), n);
    let mut out = Vec::new();
    for z in crate::linalg::kernel(&k, &ab)? {
        let x: Vec<_> = z[..n].to_vec();
        if red.insert(x.clone()) {
            out.push(x);
        }
    }
    Ok(out)
}

/// Generators of a graded subspace of `Hom(−, ⊕ X_j⟨s_j⟩)` given degree by degree at the test
/// objects by `piece` (a spanning set of vectors in [`space`]): in each degree, the subspace
/// modulo what earlier generators produce. It is a submodule when the pieces are closed under
/// precomposition; then the generators generate it in the degrees of the window.
pub fn subspace_generators<A: Additive>(
    a: &A,
    src: &[Summand<A::Obj>],
    tests: &[A::Obj],
    w: Window,
    mut piece: impl FnMut(&A::Obj, i64, &Space<A::Mor>) -> Result<Vec<Vec<<A::Field as Ring>::Elem>>>,
) -> Result<Vec<Generator<A::Obj, A::Mor>>> {
    let k = a.field().clone();
    let mut gens: Vec<Generator<A::Obj, A::Mor>> = Vec::new();
    for d in w.lo..=w.hi {
        for t in tests {
            let sp = space(a, t, d, src)?;
            let n = sp.basis.len();
            if n == 0 {
                continue;
            }
            let sub = piece(t, d, &sp)?;
            if sub.is_empty() {
                continue;
            }
            // what the generators found so far produce in Hom(T⟨d⟩, X)
            let mut span = Reducer::new(k.clone(), n);
            for g in &gens {
                let Some((lo, hi)) = a.hom_degrees(t, &g.obj)? else {
                    continue;
                };
                let e = d - g.degree;
                if e < lo || hi.is_some_and(|h| e > h) {
                    continue;
                }
                for b in a.hom_basis(t, &g.obj, e)? {
                    let col: Vec<A::Mor> = g.column.iter().map(|f| a.compose(f, &b)).collect();
                    span.insert(column_coords(a, t, d, src, &sp, &col)?);
                }
            }
            for z in sub {
                if span.insert(z.clone()) {
                    gens.push(Generator {
                        obj: t.clone(),
                        degree: d,
                        column: column_of(a, &sp, src.len(), &z),
                    });
                }
            }
        }
    }
    Ok(gens)
}

/// [`kernel_generators`] relative to `rel = (R, φ : R → Y)`: generators of the maps into `X`
/// whose composite with `m` factors through `φ` (the kernel of `X → coker φ`).
#[allow(clippy::too_many_arguments)]
pub fn relative_kernel_generators<A: Additive>(
    a: &A,
    src: &[Summand<A::Obj>],
    tgt: &[Summand<A::Obj>],
    m: &Matrix<A::Mor>,
    rel: Relations<'_, A>,
    tests: &[A::Obj],
    max: Option<i64>,
    default_span: i64,
) -> Result<KernelGenerators<A>> {
    let Some(w) = window(range_over_tests(a, tests, src)?, max, default_span) else {
        return Ok((vec![], None));
    };
    let gens = subspace_generators(a, src, tests, w, |t, d, sp| {
        relative_kernel_piece(a, t, d, sp, tgt, m, rel)
    })?;
    Ok((gens, Some(w)))
}
