//! Operations on bounded complexes (`docs/complexes.md`): checking complexes, chain maps, and
//! homotopies; homology at test objects; kernel generators for building resolutions step by
//! step; and bases of Hom spaces. They run in weighted KLR contexts, whose objects are the right
//! projectives `e(ℓ) A`.

use crate::complex::{
    self, Additive, Complex, Failure, Window, check_chain_map, check_complex, check_homotopy,
    complex_json, fmt_pos, homology_piece, kernel_generators, parse_complex, parse_map, pos_json,
    range_over_tests, window,
};
use crate::context::ContextSpec;
use crate::error::{Error, Result};
use crate::ops::{RingVisitor, with_ring};
use crate::ring::Ring;
use serde_json::{Value, json};

/// The operations of this module.
pub const OPS: [&str; 6] = [
    "complex.check",
    "chainmap.check",
    "homotopy.check",
    "complex.homology",
    "complex.kernel",
    "hom.basis",
];

/// How many degrees are searched above the lowest when a Hom space is unbounded above and no
/// `max_degree` is given. Results say when this cut applies.
pub const DEFAULT_SPAN: i64 = 6;

/// How many failures a check lists (it reports how many there are in all).
const SHOWN_FAILURES: usize = 20;

pub fn run(op: &str, input: &Value) -> Result<Value> {
    let context = input
        .get("context")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::invalid("the request needs a context"))?;
    let spec = ContextSpec::parse(context)?;
    if spec.presentation != "klrw" {
        return Err(Error::precondition(format!(
            "complexes are available in weighted KLR contexts (klrw), not in {}",
            spec.presentation
        )));
    }
    let ring = spec.ring.clone();
    with_ring(&ring, Op { op, input, spec })
}

struct Op<'a> {
    op: &'a str,
    input: &'a Value,
    spec: ContextSpec,
}

impl RingVisitor for Op<'_> {
    type Out = Value;
    fn visit<R: Ring>(self, base: R) -> Result<Value> {
        let k = crate::rep::klrw::Klrw::new(&self.spec, base)?;
        dispatch(&k, self.op, self.input)
    }
}

fn get<'v>(input: &'v Value, key: &str) -> Result<&'v Value> {
    input
        .get(key)
        .filter(|v| !v.is_null())
        .ok_or_else(|| Error::invalid(format!("the request needs '{key}'")))
}

fn opt_int(input: &Value, key: &str) -> Result<Option<i64>> {
    match input.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_i64()
            .map(Some)
            .ok_or_else(|| Error::invalid(format!("'{key}' is an integer"))),
    }
}

fn opt_uint(input: &Value, key: &str, default: usize) -> Result<usize> {
    match input.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(v) => v
            .as_u64()
            .map(|n| n as usize)
            .ok_or_else(|| Error::invalid(format!("'{key}' is a nonnegative integer"))),
    }
}

fn axis_of<O, M>(input: &Value, c: &Complex<O, M>) -> Result<usize> {
    let axis = opt_uint(input, "axis", 0)?;
    if axis >= c.dim {
        return Err(Error::invalid(format!(
            "axis {axis} does not exist: the complex has dimension {} (axes are numbered from 0)",
            c.dim
        )));
    }
    Ok(axis)
}

fn verdict<A: Additive>(a: &A, failures: &[Failure], notes: Vec<String>) -> Value {
    let mut prov = a.provenance();
    if !notes.is_empty() {
        prov["notes"] = json!(notes);
    }
    json!({
        "holds": failures.is_empty(),
        "failures": failures.iter().take(SHOWN_FAILURES).map(Failure::json).collect::<Vec<_>>(),
        "failure_count": failures.len(),
        "certified_by": "certificate",
        "provenance": prov,
    })
}

fn window_json(w: &Option<Window>) -> Value {
    match w {
        None => Value::Null,
        Some(w) => json!({ "from": w.lo, "to": w.hi, "truncated": w.truncated }),
    }
}

fn window_note(w: &Option<Window>, what: &str) -> Option<String> {
    let w = w.as_ref()?;
    w.truncated.then(|| {
        format!(
            "{what} computed in degrees {} to {} only: the Hom spaces are not bounded above \
             there, so higher degrees were not searched (raise max_degree to search further)",
            w.lo, w.hi
        )
    })
}

fn column_json<A: Additive>(a: &A, col: &[A::Mor]) -> Result<Value> {
    let mut out = Vec::with_capacity(col.len());
    for f in col {
        out.push(if a.is_zero(f)? {
            Value::Null
        } else {
            json!(a.fmt_mor(f)?)
        });
    }
    Ok(Value::Array(out))
}

/// Parses a complex and refuses to go on if it is not one: homology and kernels of a matrix
/// sequence that is not a complex would mislead.
fn valid_complex<A: Additive>(a: &A, v: &Value, name: &str) -> Result<Complex<A::Obj, A::Mor>> {
    let c = parse_complex(a, v)?;
    let fs = check_complex(a, &c)?;
    if let Some(f) = fs.first() {
        return Err(Error::invalid(format!(
            "'{name}' is not a complex ({} failure{}): {}: {} ({}); run complex.check for all \
             of them",
            fs.len(),
            if fs.len() == 1 { "" } else { "s" },
            f.location,
            f.detail,
            f.condition
        )));
    }
    Ok(c)
}

fn dispatch<A: Additive>(a: &A, op: &str, input: &Value) -> Result<Value> {
    match op {
        "complex.check" => {
            let c = parse_complex(a, get(input, "complex")?)?;
            let fs = check_complex(a, &c)?;
            let mut v = verdict(a, &fs, vec![]);
            v["conventions"] = c.conv.to_json();
            Ok(v)
        }
        "chainmap.check" => {
            let c = parse_complex(a, get(input, "source")?)?;
            let d = parse_complex(a, get(input, "target")?)?;
            let zero = vec![0; c.dim];
            let f = parse_map(a, get(input, "map")?, &c, &d, &zero, "map")?;
            Ok(verdict(a, &check_chain_map(a, &c, &d, &f)?, vec![]))
        }
        "homotopy.check" => {
            let c = parse_complex(a, get(input, "source")?)?;
            let d = parse_complex(a, get(input, "target")?)?;
            let zero = vec![0; c.dim];
            let f = parse_map(a, get(input, "f")?, &c, &d, &zero, "f")?;
            let g = match input.get("g") {
                None | Some(Value::Null) => complex::Map::new(),
                Some(g) => parse_map(a, g, &c, &d, &zero, "g")?,
            };
            let back = vec![-c.conv.step(); c.dim];
            let h = parse_map(a, get(input, "h")?, &c, &d, &back, "h")?;
            Ok(verdict(a, &check_homotopy(a, &c, &d, &f, &g, &h)?, vec![]))
        }
        "complex.homology" => homology(a, input),
        "complex.kernel" => kernel(a, input),
        "hom.basis" => hom_basis(a, input),
        _ => Err(Error::invalid(format!("unknown operation '{op}'"))),
    }
}

fn homology<A: Additive>(a: &A, input: &Value) -> Result<Value> {
    a.linear_algebra()?;
    let c = valid_complex(a, get(input, "complex")?, "complex")?;
    let axis = axis_of(input, &c)?;
    let max = opt_int(input, "max_degree")?;
    let reps = opt_uint(input, "representatives", 0)?;
    let positions: Vec<complex::Pos> = match input.get("at") {
        None | Some(Value::Null) => c.positions().into_iter().collect(),
        Some(p) => vec![complex::position(p, c.dim)?],
    };
    let objs: Vec<A::Obj> = c
        .terms
        .values()
        .flat_map(|ss| ss.iter().map(|s| s.obj.clone()))
        .collect();
    let tests = a.test_objects(&objs)?;
    let mut out = Vec::new();
    let mut notes = Vec::new();
    let mut exact = true;
    for p in &positions {
        let here = c.at(p);
        let w = window(range_over_tests(a, &tests, here)?, max, DEFAULT_SPAN);
        let mut pieces = Vec::new();
        if let Some(w) = w {
            for d in w.lo..=w.hi {
                for t in &tests {
                    let h = homology_piece(a, &c, p, axis, t, d, reps)?;
                    if h.dim == 0 {
                        continue;
                    }
                    let mut piece = json!({
                        "test": a.fmt_obj(t),
                        "degree": d,
                        "dimension": h.dim,
                    });
                    if reps > 0 {
                        piece["representatives"] = Value::Array(
                            h.representatives
                                .iter()
                                .map(|col| column_json(a, col))
                                .collect::<Result<_>>()?,
                        );
                    }
                    pieces.push(piece);
                }
            }
        }
        if !pieces.is_empty() || w.is_some_and(|w| w.truncated) {
            exact = false;
        }
        if let Some(n) = window_note(&w, &format!("the homology at {}", fmt_pos(p))) {
            notes.push(n);
        }
        out.push(json!({
            "at": pos_json(p),
            "window": window_json(&w),
            "zero": pieces.is_empty(),
            "pieces": pieces,
        }));
    }
    let mut prov = a.provenance();
    notes.insert(
        0,
        format!(
            "homology of Hom(T⟨d⟩, C) at the test objects T: {}",
            tests
                .iter()
                .map(|t| a.fmt_obj(t))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    );
    prov["notes"] = json!(notes);
    Ok(json!({
        "axis": axis,
        "positions": out,
        "exact": exact,
        "certified_by": "certificate",
        "provenance": prov,
    }))
}

fn kernel<A: Additive>(a: &A, input: &Value) -> Result<Value> {
    a.linear_algebra()?;
    let mut c = valid_complex(a, get(input, "complex")?, "complex")?;
    let axis = axis_of(input, &c)?;
    let p = complex::position(get(input, "at")?, c.dim)?;
    let max = opt_int(input, "max_degree")?;
    let src = c.at(&p).to_vec();
    if src.is_empty() {
        return Err(Error::invalid(format!(
            "the complex has no term at {}",
            fmt_pos(&p)
        )));
    }
    let q = c.next(&p, axis);
    let tgt = c.at(&q).to_vec();
    let m = c.diff(a, axis, &p);
    let objs: Vec<A::Obj> = src.iter().chain(&tgt).map(|s| s.obj.clone()).collect();
    let tests = a.test_objects(&objs)?;
    let (gens, w) = kernel_generators(a, &src, &tgt, &m, &tests, max, DEFAULT_SPAN)?;
    let summands: Vec<complex::Summand<A::Obj>> = gens
        .iter()
        .map(|g| complex::Summand {
            obj: g.obj.clone(),
            shift: g.degree,
        })
        .collect();
    // rows: the summands at p; columns: the generators
    let matrix: complex::Matrix<A::Mor> = (0..src.len())
        .map(|i| gens.iter().map(|g| g.column[i].clone()).collect())
        .collect();
    let back = c.prev(&p, axis);
    let mut v = json!({
        "at": pos_json(&p),
        "into": pos_json(&back),
        "summands": complex::summands_json(a, &summands),
        "matrix": complex::matrix_json(a, &matrix)?,
        "window": window_json(&w),
        "complete": !w.is_some_and(|w| w.truncated),
    });
    let mut notes = vec![
        "the generators are found degree by degree: in each degree, the kernel modulo what the \
         earlier generators produce; they generate the kernel but need not be a minimal set"
            .to_string(),
    ];
    if let Some(n) = window_note(&w, "the kernel") {
        notes.push(n);
    }
    if c.at(&back).is_empty() && c.dim == 1 && !summands.is_empty() {
        c.terms.insert(back.clone(), summands);
        c.diffs.insert((axis, back), matrix);
        v["extended"] = complex_json(a, &c)?;
    }
    let mut prov = a.provenance();
    prov["notes"] = json!(notes);
    v["certified_by"] = json!("certificate");
    v["provenance"] = prov;
    Ok(v)
}

fn hom_basis<A: Additive>(a: &A, input: &Value) -> Result<Value> {
    a.linear_algebra()?;
    let obj = |k: &str| -> Result<A::Obj> {
        let s = get(input, k)?
            .as_str()
            .ok_or_else(|| Error::invalid(format!("'{k}' is an object (a string)")))?;
        a.parse_obj(s)
    };
    let (src, tgt) = (obj("source")?, obj("target")?);
    let degrees: Vec<i64>;
    let mut w = None;
    if let Some(d) = opt_int(input, "degree")? {
        degrees = vec![d];
    } else {
        w = window(
            a.hom_degrees(&src, &tgt)?,
            opt_int(input, "max_degree")?,
            DEFAULT_SPAN,
        );
        degrees = w.map(|w| (w.lo..=w.hi).collect()).unwrap_or_default();
    }
    let mut out = Vec::new();
    for d in degrees {
        let b = a.hom_basis(&src, &tgt, d)?;
        if b.is_empty() && w.is_some() {
            continue;
        }
        out.push(json!({
            "degree": d,
            "dimension": b.len(),
            "basis": b.iter().map(|f| a.fmt_mor(f)).collect::<Result<Vec<_>>>()?,
        }));
    }
    let mut prov = a.provenance();
    if let Some(n) = window_note(&w, "the Hom space") {
        prov["notes"] = json!([n]);
    }
    Ok(json!({
        "source": a.fmt_obj(&src),
        "target": a.fmt_obj(&tgt),
        "degrees": out,
        "window": window_json(&w),
        "certified_by": "certificate",
        "provenance": prov,
    }))
}
