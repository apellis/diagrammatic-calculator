//! Operations on bounded complexes (`docs/complexes.md`): checking complexes, chain maps, and
//! homotopies; homology at test objects; kernel generators for building resolutions step by
//! step; and bases of Hom spaces. They run in weighted KLR contexts, whose objects are the right
//! projectives `e(ℓ) A`.

use crate::complex::{
    self, Additive, Complex, Failure, Window, check_chain_map, check_complex, check_homotopy,
    complex_json, fmt_pos, homology_piece, kernel_generators, parse_complex, parse_map, pos_json,
    range_over_tests, window,
};
use crate::complex_deform::{self as cd, Mask};
use crate::complex_module as cm;
use crate::context::ContextSpec;
use crate::deform::{ParamSplit, Truncated};
use crate::error::{Error, Result};
use crate::ops::{RingVisitor, with_ring};
use crate::ring::Ring;
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// The operations of this module.
pub const OPS: [&str; 24] = [
    "complex.check",
    "chainmap.check",
    "homotopy.check",
    "complex.homology",
    "complex.kernel",
    "hom.basis",
    "map.lift",
    "map.extend",
    "chainmap.solve",
    "homotopy.find",
    "complex.minimal",
    "complex.shift",
    "complex.cone",
    "complex.totalize",
    "complex.hom",
    "complex.gauss",
    "complex.euler",
    "complex.render",
    "module.check",
    "module.head",
    "resolution.start",
    "resolution.extend",
    "resolution.check",
    "complex.deform",
];

/// How many elimination steps `complex.gauss` takes at most when no `limit` is given.
pub const DEFAULT_GAUSS_LIMIT: usize = 1000;

/// How many degrees are searched above the lowest when a Hom space is unbounded above and no
/// `max_degree` is given. Results say when this cut applies.
pub const DEFAULT_SPAN: i64 = 6;

/// How many failures a check lists (it reports how many there are in all).
const SHOWN_FAILURES: usize = 20;

/// The keys a request for `op` may have: the properties of its schema's request
/// (`spec/ops/<op>.schema.json`).
pub fn request_keys(op: &str) -> Vec<String> {
    let schema = crate::ops::SCHEMAS
        .iter()
        .find(|(name, _)| *name == op)
        .map(|(_, s)| s)
        .unwrap_or_else(|| panic!("no schema for {op}"));
    let v: Value = serde_json::from_str(schema).expect("the shipped schemas are JSON");
    v["$defs"]["request"]["properties"]
        .as_object()
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default()
}

pub fn run(op: &str, input: &Value) -> Result<Value> {
    let keys = request_keys(op);
    let keys: Vec<&str> = keys.iter().map(String::as_str).collect();
    crate::keys::check(input, &keys, &format!("a {op} request"))?;
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
        if self.op == "complex.deform" {
            let Some((h, n)) = &self.spec.truncate else {
                return Err(Error::precondition(
                    "complex.deform works order by order in a parameter: give the context \
                     truncate=h^N (and hdeg=d for the degree of h)",
                ));
            };
            let t = Truncated::new(&k, h, *n, self.spec.param_degree.unwrap_or(0))?;
            return deform_op(&t, self.input);
        }
        match &self.spec.truncate {
            None => dispatch(&k, self.op, self.input),
            Some((h, n)) => {
                let t = Truncated::new(&k, h, *n, self.spec.param_degree.unwrap_or(0))?;
                dispatch(&t, self.op, self.input)
            }
        }
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
            let (c, notes) = complex::parse_complex_noted(a, get(input, "complex")?)?;
            let fs = check_complex(a, &c)?;
            let mut v = verdict(a, &fs, notes);
            v["conventions"] = c.conv.to_json();
            // with the inferred objects and shifts filled in
            v["complex"] = complex_json(a, &c)?;
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
        "map.lift" | "map.extend" => factor(a, op, input),
        "chainmap.solve" => chainmap_solve(a, input),
        "homotopy.find" => homotopy_find(a, input),
        "complex.minimal" => minimal(a, input),
        "complex.shift" => shift(a, input),
        "complex.cone" => cone(a, input),
        "complex.totalize" => totalize(a, input),
        "complex.hom" => hom_complex(a, input),
        "complex.gauss" => gauss(a, input),
        "complex.euler" => euler(a, input),
        "complex.render" => render(a, input),
        "module.check" => module_check(a, input),
        "module.head" => module_head(a, input),
        "resolution.start" => resolution_start(a, input),
        "resolution.extend" => resolution_extend(a, input),
        "resolution.check" => resolution_check(a, input),
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
    let (gens, w, dropped) = kernel_generators(a, &src, &tgt, &m, &tests, max, DEFAULT_SPAN)?;
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
         earlier generators produce; then a generator in the submodule generated by the others \
         is dropped, so they generate the kernel and none is redundant"
            .to_string(),
    ];
    notes.extend(complex::dropped_note(dropped, "the kernel"));
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

fn with_prov(mut v: Value, prov: Value) -> Value {
    v["certified_by"] = json!("certificate");
    v["provenance"] = prov;
    v
}

fn prov_notes<A: Additive>(a: &A, notes: Vec<String>) -> Value {
    let mut prov = a.provenance();
    if !notes.is_empty() {
        prov["notes"] = json!(notes);
    }
    prov
}

/// A map as `[{at, matrix}]`, omitting zero components.
fn map_json<A: Additive>(a: &A, m: &complex::Map<A::Mor>) -> Result<Value> {
    let mut out = Vec::new();
    for (p, mat) in m {
        let mut zero = true;
        for f in mat.iter().flatten() {
            if !a.is_zero(f)? {
                zero = false;
                break;
            }
        }
        if !zero {
            out.push(json!({ "at": pos_json(p), "matrix": complex::matrix_json(a, mat)? }));
        }
    }
    Ok(Value::Array(out))
}

fn summands_of<A: Additive>(
    a: &A,
    input: &Value,
    key: &str,
) -> Result<Vec<complex::Summand<A::Obj>>> {
    complex::parse_summands(a, get(input, key)?, &format!("'{key}'"))
}

/// `map.lift` (find `h : X → Y` with `g h = f`) and `map.extend` (find `h : Y → Z` with
/// `h g = f`), for degree-zero maps between finite sums.
fn factor<A: Additive>(a: &A, op: &str, input: &Value) -> Result<Value> {
    a.linear_algebra()?;
    let x = summands_of(a, input, "source")?;
    let y = summands_of(a, input, "middle")?;
    let z = summands_of(a, input, "target")?;
    let f = complex::parse_matrix(a, get(input, "f")?, z.len(), x.len(), "'f'")?;
    let lifting = op == "map.lift";
    let g = if lifting {
        complex::parse_matrix(a, get(input, "g")?, z.len(), y.len(), "'g'")?
    } else {
        complex::parse_matrix(a, get(input, "g")?, y.len(), x.len(), "'g'")?
    };
    let mut fs = Vec::new();
    complex::check_entries(a, &x, &z, &f, 0, "'f'", &Value::Null, &mut fs);
    if lifting {
        complex::check_entries(a, &y, &z, &g, 0, "'g'", &Value::Null, &mut fs);
    } else {
        complex::check_entries(a, &x, &y, &g, 0, "'g'", &Value::Null, &mut fs);
    }
    if let Some(e) = fs.first() {
        return Err(Error::invalid(format!(
            "{}: {} ({})",
            e.location, e.detail, e.condition
        )));
    }
    let sol = if lifting {
        crate::complex_solve::lift(a, &x, &y, &z, &f, &g)?
    } else {
        crate::complex_solve::extend(a, &x, &y, &z, &f, &g)?
    };
    let what = if lifting { "g h = f" } else { "h g = f" };
    let v = match sol {
        None => json!({ "exists": false, "solution": Value::Null, "freedom": 0 }),
        Some(s) => json!({
            "exists": true,
            "solution": complex::matrix_json(a, &s.particular[0])?,
            "freedom": s.kernel.len(),
        }),
    };
    Ok(with_prov(
        v,
        prov_notes(
            a,
            vec![format!(
                "solved {what} as a linear system in the degree-zero Hom spaces; 'freedom' is \
                 the dimension of the solutions of the homogeneous system"
            )],
        ),
    ))
}

fn chainmap_solve<A: Additive>(a: &A, input: &Value) -> Result<Value> {
    a.linear_algebra()?;
    let c = valid_complex(a, get(input, "source")?, "source")?;
    let d = valid_complex(a, get(input, "target")?, "target")?;
    if c.dim != d.dim || c.conv.indexing != d.conv.indexing {
        return Err(Error::invalid(
            "the source and target have different dimensions or indexing",
        ));
    }
    let zero = vec![0; c.dim];
    let fixed = match input.get("fixed") {
        None | Some(Value::Null) => complex::Map::new(),
        Some(v) => parse_map(a, v, &c, &d, &zero, "fixed")?,
    };
    for (p, m) in &fixed {
        let mut fs = Vec::new();
        let loc = format!("the component of 'fixed' at {}", fmt_pos(p));
        complex::check_entries(a, c.at(p), d.at(p), m, 0, &loc, &Value::Null, &mut fs);
        if let Some(e) = fs.first() {
            return Err(Error::invalid(format!("{}: {}", e.location, e.detail)));
        }
    }
    let (free, sol) = crate::complex_solve::solve_chain_map(a, &c, &d, &fixed)?;
    let mut v = json!({
        "free_positions": free.iter().map(|p| pos_json(p)).collect::<Vec<_>>(),
    });
    let mut notes = vec![
        "unknown components at every position where both complexes are nonzero and no \
         component is fixed; d F = F d imposed along every axis; 'freedom' is the \
         dimension of the chain maps vanishing on the fixed components"
            .to_string(),
    ];
    match sol {
        None => {
            v["exists"] = json!(false);
            v["map"] = Value::Null;
            v["freedom"] = json!(0);
        }
        Some(s) => {
            let mut m = fixed.clone();
            for (p, x) in free.iter().zip(s.particular) {
                m.insert(p.clone(), x);
            }
            v["exists"] = json!(true);
            v["map"] = map_json(a, &m)?;
            v["freedom"] = json!(s.kernel.len());
            let basis_n = opt_uint(input, "basis", 0)?;
            if basis_n > 0 {
                let mut out = Vec::new();
                for k in s.kernel.iter().take(basis_n) {
                    let km: complex::Map<A::Mor> =
                        free.iter().cloned().zip(k.iter().cloned()).collect();
                    out.push(map_json(a, &km)?);
                }
                v["homogeneous_basis"] = Value::Array(out);
                if basis_n < s.kernel.len() {
                    notes.push(format!(
                        "'homogeneous_basis' lists {basis_n} of the {} basis vectors of the \
                         homogeneous solutions ('basis' asked for at most {basis_n})",
                        s.kernel.len()
                    ));
                }
            }
        }
    }
    Ok(with_prov(v, prov_notes(a, notes)))
}

fn homotopy_find<A: Additive>(a: &A, input: &Value) -> Result<Value> {
    a.linear_algebra()?;
    let c = valid_complex(a, get(input, "source")?, "source")?;
    let d = valid_complex(a, get(input, "target")?, "target")?;
    if c.dim != 1 || d.dim != 1 || c.conv.indexing != d.conv.indexing {
        return Err(Error::precondition(
            "homotopies are found between one-dimensional complexes with the same indexing; \
             totalize first",
        ));
    }
    let zero = vec![0];
    let f = parse_map(a, get(input, "f")?, &c, &d, &zero, "f")?;
    let g = match input.get("g") {
        None | Some(Value::Null) => complex::Map::new(),
        Some(g) => parse_map(a, g, &c, &d, &zero, "g")?,
    };
    let mut fg = f.clone();
    for (p, gm) in &g {
        let base = fg
            .remove(p)
            .unwrap_or_else(|| complex::zero_matrix(a, d.at(p).len(), c.at(p).len()));
        fg.insert(p.clone(), complex::mat_add(a, &base, gm, -1));
    }
    let (free, sol) = crate::complex_solve::find_homotopy(a, &c, &d, &fg)?;
    let v = match sol {
        None => json!({ "exists": false, "h": Value::Null, "freedom": 0 }),
        Some(s) => {
            let h: complex::Map<A::Mor> = free.iter().cloned().zip(s.particular).collect();
            json!({ "exists": true, "h": map_json(a, &h)?, "freedom": s.kernel.len() })
        }
    };
    Ok(with_prov(
        v,
        prov_notes(
            a,
            vec![
                "solved f − g = d h + h d with h(p) : C(p) → D(p − step) as a linear system in \
                 the degree-zero Hom spaces"
                    .into(),
            ],
        ),
    ))
}

fn minimal<A: Additive>(a: &A, input: &Value) -> Result<Value> {
    a.linear_algebra()?;
    let c = valid_complex(a, get(input, "complex")?, "complex")?;
    let found = crate::complex_solve::check_minimal(a, &c)?;
    let fs: Vec<Failure> = found.iter().map(|(f, _)| f.clone()).collect();
    let mut v = verdict(
        a,
        &fs,
        vec![
            "a differential d : X → Y is in the radical when the left ideal {g d : g : Y → X} of \
             End(X) is nilpotent; a complex of projectives over a finite-dimensional algebra is \
             minimal (has no contractible summand) exactly when all its differentials are"
                .into(),
        ],
    );
    let mut failures = Vec::new();
    for (f, w) in found.iter().take(SHOWN_FAILURES) {
        let mut j = f.json();
        if let Some(w) = w {
            j["witness"] = complex::matrix_json(a, w)?;
        }
        failures.push(j);
    }
    v["failures"] = Value::Array(failures);
    Ok(v)
}

fn shift<A: Additive>(a: &A, input: &Value) -> Result<Value> {
    let c = valid_complex(a, get(input, "complex")?, "complex")?;
    let by = match input.get("by") {
        None | Some(Value::Null) => vec![0; c.dim],
        Some(v) => complex::position(v, c.dim)?,
    };
    let grading = opt_int(input, "grading")?.unwrap_or(0);
    let s = crate::complex_build::shift(a, &c, &by, grading);
    Ok(with_prov(
        json!({ "complex": complex_json(a, &s)? }),
        a.provenance(),
    ))
}

fn cone<A: Additive>(a: &A, input: &Value) -> Result<Value> {
    let c = valid_complex(a, get(input, "source")?, "source")?;
    let d = valid_complex(a, get(input, "target")?, "target")?;
    let zero = vec![0; c.dim];
    let f = parse_map(a, get(input, "map")?, &c, &d, &zero, "map")?;
    if let Some(e) = check_chain_map(a, &c, &d, &f)?.first() {
        return Err(Error::invalid(format!(
            "'map' is not a chain map: {}: {} ({}); run chainmap.check for all failures",
            e.location, e.detail, e.condition
        )));
    }
    let k = crate::complex_build::cone(a, &c, &d, &f)?;
    Ok(with_prov(
        json!({ "complex": complex_json(a, &k)? }),
        prov_notes(
            a,
            vec![
                "Cone(F)(p) = C(p + step) ⊕ D(p) (the summands of C first), with differential \
                 [[−d_C, 0], [F, d_D]]"
                    .into(),
            ],
        ),
    ))
}

fn totalize<A: Additive>(a: &A, input: &Value) -> Result<Value> {
    let c = valid_complex(a, get(input, "complex")?, "complex")?;
    let (t, blocks) = crate::complex_build::totalize(a, &c)?;
    let blocks_json: Vec<Value> = blocks
        .iter()
        .map(|(n, bs)| {
            json!({
                "at": n,
                "blocks": bs.iter().map(|(p, o)| json!({ "from": pos_json(p), "first": o }))
                    .collect::<Vec<_>>(),
            })
        })
        .collect();
    let rule = match c.conv.squares {
        complex::Squares::Commute => {
            "the differential along axis a is multiplied by (−1)^(p_0 + … + p_(a−1))"
        }
        complex::Squares::Anticommute => "the squares anticommute, so no signs are added",
    };
    Ok(with_prov(
        json!({ "complex": complex_json(a, &t)?, "blocks": blocks_json }),
        prov_notes(
            a,
            vec![format!(
                "Tot(n) is the sum of the terms at the positions with coordinate sum n, in \
                 increasing order of position; {rule}"
            )],
        ),
    ))
}

fn hom_complex<A: Additive>(a: &A, input: &Value) -> Result<Value> {
    a.linear_algebra()?;
    let c = valid_complex(a, get(input, "source")?, "source")?;
    let d = valid_complex(a, get(input, "target")?, "target")?;
    let (degrees, w) = match opt_int(input, "degree")? {
        Some(e) => (vec![e], None),
        None => {
            let w = crate::complex_build::hom_window(
                a,
                &c,
                &d,
                opt_int(input, "max_degree")?,
                DEFAULT_SPAN,
            )?;
            (w.map(|w| (w.lo..=w.hi).collect()).unwrap_or_default(), w)
        }
    };
    let mut out = Vec::new();
    for e in degrees {
        let rows = crate::complex_build::hom_cohomology(a, &c, &d, e)?;
        let nonzero: Vec<Value> = rows
            .iter()
            .filter(|(_, _, h)| *h > 0)
            .map(|(n, dn, h)| json!({ "n": n, "cochains": dn, "dimension": h }))
            .collect();
        if nonzero.is_empty() && w.is_some() {
            continue;
        }
        out.push(json!({ "degree": e, "cohomology": nonzero }));
    }
    let mut notes = vec![
        "Hom^n is the product over p of the degree-e maps C(p) → D(p + n), with δ f = d_D f − \
         (−1)^n f d_C; H^0 in degree e is the space of chain maps C⟨e⟩ → D up to homotopy. When \
         C is a projective resolution of M, H^n is Ext^n(M, D) only for n below the length of \
         the resolution that was built"
            .to_string(),
    ];
    if let Some(n) = window_note(&w, "the Hom complex") {
        notes.push(n);
    }
    Ok(with_prov(
        json!({ "degrees": out, "window": window_json(&w) }),
        prov_notes(a, notes),
    ))
}

fn gauss<A: Additive>(a: &A, input: &Value) -> Result<Value> {
    a.linear_algebra()?;
    let cv = get(input, "complex")?;
    let c = valid_complex(a, cv, "complex")?;
    let limit = opt_uint(input, "limit", DEFAULT_GAUSS_LIMIT)?;
    let resolves = cv.get("resolves").is_some_and(|r| !r.is_null());
    let (out, steps, finished) = if resolves {
        let (r, _) = cm::parse_resolution(a, cv)?;
        let (r, steps, finished) = cm::gauss_resolution(a, &r, limit)?;
        (cm::resolution_json(a, &r)?, steps, finished)
    } else {
        let (r, steps, finished) = crate::complex_build::gauss(a, &c, limit)?;
        (complex_json(a, &r)?, steps, finished)
    };
    let mut notes = vec![
        "each step removes a summand pair joined by an invertible entry φ of a differential, \
         replacing the rest of that differential by ε − γ φ⁻¹ δ; the result is homotopy \
         equivalent to the input. Only invertible single entries are used, so the result need \
         not be minimal: check with complex.minimal"
            .to_string(),
    ];
    if !finished {
        notes.push(format!(
            "stopped after {limit} steps (the limit); more eliminations may be possible"
        ));
    }
    Ok(with_prov(
        json!({
            "complex": out,
            "eliminations": steps.iter().map(|s| json!({
                "from": s.from, "source": s.source, "target": s.target
            })).collect::<Vec<_>>(),
            "finished": finished,
        }),
        prov_notes(a, notes),
    ))
}

fn euler<A: Additive>(a: &A, input: &Value) -> Result<Value> {
    let c = valid_complex(a, get(input, "complex")?, "complex")?;
    Ok(euler_of(a, &c))
}

/// The Euler characteristic of a sequence of terms. It depends only on the terms, so a cell
/// shows it even while `d ∘ d ≠ 0`.
fn euler_of<A: Additive>(a: &A, c: &Complex<A::Obj, A::Mor>) -> Value {
    let classes = crate::complex_build::euler(a, c);
    let terms: Vec<Value> = classes
        .iter()
        .map(|(o, cs)| {
            json!({
                "object": a.fmt_obj(o),
                "coefficient": crate::complex_build::laurent_text(cs),
                "powers": cs.iter().map(|(e, k)| json!([e, k])).collect::<Vec<_>>(),
            })
        })
        .collect();
    let text = if classes.is_empty() {
        "0".to_string()
    } else {
        classes
            .iter()
            .map(|(o, cs)| {
                format!(
                    "({})[{}]",
                    crate::complex_build::laurent_text(cs),
                    a.fmt_obj(o)
                )
            })
            .collect::<Vec<_>>()
            .join(" + ")
    };
    with_prov(
        json!({ "terms": terms, "text": text }),
        prov_notes(
            a,
            vec![
                "Σ_p (−1)^(p_0 + … + p_(n−1)) Σ q^s [X] over the summands X⟨s⟩ of C(p), with \
                 isotopic objects grouped; the classes [X] are not reduced further"
                    .into(),
            ],
        ),
    )
}

fn render<A: Additive>(a: &A, input: &Value) -> Result<Value> {
    let c = valid_complex(a, get(input, "complex")?, "complex")?;
    let tex = crate::complex_build::tikz(a, &c)?;
    Ok(with_prov(
        json!({ "format": "tikz-cd", "tikz": tex }),
        prov_notes(a, vec!["needs \\usepackage{tikz-cd} and amsmath".into()]),
    ))
}

/// The value of a workspace cell `complex name := {…}`: the complex with every object and shift
/// filled in, whether it is a complex, its Euler characteristic, and, when it is a complex and
/// linear algebra is available, its homology with a summary for building a resolution.
pub(crate) fn complex_cell<A: Additive>(a: &A, v: &Value) -> Result<Value> {
    if v.get("resolves").is_some_and(|r| !r.is_null()) {
        return resolution_cell(a, v);
    }
    let (c, mut notes) = complex::parse_complex_noted(a, v)?;
    let cj = complex_json(a, &c)?;
    let fs = check_complex(a, &c)?;
    let mut out = json!({
        "complex": cj,
        "check": verdict(a, &fs, vec![]),
        "euler": euler_of(a, &c),
    });
    if !fs.is_empty() {
        notes.push("homology is computed once the check passes".into());
    } else if let Err(e) = a.linear_algebra() {
        notes.push(format!("no homology: {}", e.message));
    } else {
        let h = homology(a, &json!({ "complex": cj }))?;
        if c.dim == 1 && !c.terms.is_empty() {
            out["resolution"] = resolution_summary(a, &c, &h)?;
        }
        out["homology"] = h;
    }
    let mut prov = a.provenance();
    prov["notes"] = json!(notes);
    out["provenance"] = prov;
    Ok(out)
}

/// For a one-dimensional complex `P_n → … → P_0` read as a projective resolution of the
/// homology `M` at its last position (along the differentials): `M`'s graded dimensions at each
/// test object, where homology is still to be killed, and whether it is a resolution.
fn resolution_summary<A: Additive>(a: &A, c: &Complex<A::Obj, A::Mor>, h: &Value) -> Result<Value> {
    let step = c.conv.step();
    let support: Vec<i64> = c
        .terms
        .iter()
        .filter(|(_, ss)| !ss.is_empty())
        .map(|(p, _)| p[0])
        .collect();
    let (Some(&lo), Some(&hi)) = (support.first(), support.last()) else {
        return Ok(Value::Null);
    };
    let (start, end) = if step > 0 { (lo, hi) } else { (hi, lo) };
    let positions = h["positions"].as_array().cloned().unwrap_or_default();
    let at = |p: i64| positions.iter().find(|x| x["at"] == json!(p));
    let truncated = |x: &Value| x["window"]["truncated"] == json!(true);
    let mut module: BTreeMap<String, BTreeMap<i64, i64>> = BTreeMap::new();
    if let Some(x) = at(end) {
        for piece in x["pieces"].as_array().into_iter().flatten() {
            let t = piece["test"].as_str().unwrap_or_default().to_string();
            let d = piece["degree"].as_i64().unwrap_or(0);
            let n = piece["dimension"].as_i64().unwrap_or(0);
            *module.entry(t).or_default().entry(d).or_default() += n;
        }
    }
    let total: i64 = module.values().flat_map(|m| m.values()).sum();
    let mut gaps = Vec::new();
    for p in support.iter().copied().filter(|&p| p != start && p != end) {
        if at(p).is_some_and(|x| x["zero"] != json!(true)) {
            gaps.push(p);
        }
    }
    let open = start != end && at(start).is_some_and(|x| x["zero"] != json!(true));
    let complete = positions.iter().all(|x| !truncated(x));
    let resolved = gaps.is_empty() && !open;
    let mut out = json!({
        "module_at": end,
        "module": module.iter().map(|(t, m)| json!({
            "test": t,
            "dimensions": crate::complex_build::laurent_text(m),
            "powers": m.iter().map(|(e, k)| json!([e, k])).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "module_dimension": total,
        "length": (end - start).abs(),
        "next_at": if open { json!(start) } else { Value::Null },
        "not_exact_at": gaps,
        "resolved": resolved,
        "complete": complete,
    });
    if resolved {
        out["minimal"] = json!(crate::complex_solve::check_minimal(a, c)?.is_empty());
    }
    Ok(out)
}

/// The value of a complex cell that resolves a module: the filled resolution and its check.
fn resolution_cell<A: Additive>(a: &A, v: &Value) -> Result<Value> {
    let (r, mut notes) = cm::parse_resolution(a, v)?;
    let mut out = json!({ "complex": cm::resolution_json(a, &r)? });
    match a.linear_algebra() {
        Err(e) => notes.push(format!("no check: {}", e.message)),
        Ok(()) => {
            let (check, cnotes) = report_value(a, &r, None)?;
            notes.extend(cnotes);
            out["resolution"] = check;
        }
    }
    out["provenance"] = prov_notes(a, notes);
    Ok(out)
}

/// In a workspace, `resolves.module` may name an earlier `module` cell: this replaces the name
/// by that cell's module (the latest one of that name before cell `i`).
pub(crate) fn resolve_module_name(
    ws: &crate::workspace::Workspace,
    i: usize,
    complex: &Value,
) -> Result<Value> {
    let Some(name) = complex
        .get("resolves")
        .and_then(|r| r.get("module"))
        .and_then(Value::as_str)
    else {
        return Ok(complex.clone());
    };
    let found = ws.cells[..i].iter().rev().find_map(|c| match &c.body {
        crate::workspace::CellBody::Module { name: n, module } if n == name => Some(module),
        _ => None,
    });
    let module = found.ok_or_else(|| {
        Error::invalid(format!(
            "'resolves.module' names '{name}', but no module cell of that name comes before \
             this one"
        ))
    })?;
    let mut out = complex.clone();
    out["resolves"]["module"] = module.clone();
    Ok(out)
}

/// In a workspace, the `source` and `target` of a chain map may name earlier `complex` cells:
/// this replaces each name by that cell's complex (the latest one of that name before cell
/// `i`), without what it resolves.
pub(crate) fn resolve_complex_names(
    ws: &crate::workspace::Workspace,
    i: usize,
    map: &Value,
) -> Result<Value> {
    let mut out = map.clone();
    for key in ["source", "target"] {
        let Some(name) = map.get(key).and_then(Value::as_str) else {
            continue;
        };
        let j = ws.cells[..i]
            .iter()
            .rposition(|c| {
                matches!(&c.body, crate::workspace::CellBody::Complex { name: n, .. } if n == name)
            })
            .ok_or_else(|| {
                Error::invalid(format!(
                    "'{key}' names '{name}', but no complex cell of that name comes before this                      one"
                ))
            })?;
        let crate::workspace::CellBody::Complex { complex, .. } = &ws.cells[j].body else {
            unreachable!()
        };
        let mut c = resolve_module_name(ws, j, complex)?;
        if let Some(o) = c.as_object_mut() {
            o.remove("resolves");
        }
        out[key] = c;
    }
    Ok(out)
}

/// The value of a workspace cell `chainmap name := {…}`: its source and target filled in, its
/// nonzero components, and whether it commutes with the differentials.
pub(crate) fn chainmap_cell<A: Additive>(a: &A, v: &Value) -> Result<Value> {
    crate::keys::check(v, &["source", "target", "map"], "a chain map")?;
    let c = parse_complex(a, get(v, "source")?)?;
    let d = parse_complex(a, get(v, "target")?)?;
    if c.dim != d.dim {
        return Err(Error::invalid(format!(
            "the source has dimension {} and the target {}: a chain map needs equal dimensions",
            c.dim, d.dim
        )));
    }
    let zero = vec![0; c.dim];
    let f = match v.get("map") {
        None | Some(Value::Null) => complex::Map::new(),
        Some(m) => parse_map(a, m, &c, &d, &zero, "map")?,
    };
    let fs = check_chain_map(a, &c, &d, &f)?;
    Ok(json!({
        "source": complex_json(a, &c)?,
        "target": complex_json(a, &d)?,
        "map": map_json(a, &f)?,
        "check": verdict(a, &fs, vec![]),
        "provenance": a.provenance(),
    }))
}

/// The value of a workspace cell `module name := {…}`: the module filled in, its check, and its
/// graded dimensions at the test objects when linear algebra is available.
pub(crate) fn module_cell<A: Additive>(a: &A, v: &Value) -> Result<Value> {
    module_check(a, &json!({ "module": v }))
}

// ---------------------------------------------------------------------------------------------
// Modules and resolutions of them (`docs/complexes.md`, "Resolving a module")

fn module_test_objects<A: Additive>(a: &A, m: &cm::Module<A::Obj, A::Mor>) -> Result<Vec<A::Obj>> {
    a.test_objects(&cm::module_objects(m))
}

fn module_check<A: Additive>(a: &A, input: &Value) -> Result<Value> {
    let (m, mut notes) = cm::parse_module_noted(a, get(input, "module")?)?;
    let fs = cm::check_module(a, &m);
    let mut dims = None;
    if fs.is_empty() && a.linear_algebra().is_ok() {
        let tests = module_test_objects(a, &m)?;
        let max = opt_int(input, "max_degree")?;
        let (d, w) = cm::module_dims(a, &m, &tests, max, DEFAULT_SPAN)?;
        notes.extend(window_note(&w, "the dimensions"));
        dims = Some((d, w));
    }
    let mut v = verdict(a, &fs, notes);
    v["module"] = cm::module_json(a, &m)?;
    if let Some((d, w)) = dims {
        v["dimensions"] = cm::dims_json(&d);
        v["dimension"] = json!(cm::dims_total(&d));
        v["window"] = window_json(&w);
        v["complete"] = json!(!w.is_some_and(|w| w.truncated));
    }
    Ok(v)
}

/// The summands to take the head of: `object` (with optional `shift`) or `summands`.
fn head_summands<A: Additive>(a: &A, input: &Value) -> Result<Vec<complex::Summand<A::Obj>>> {
    match (input.get("object"), input.get("summands")) {
        (Some(o), None) if !o.is_null() => {
            let s = o
                .as_str()
                .ok_or_else(|| Error::invalid("'object' is an object (a string)"))?;
            Ok(vec![complex::Summand {
                obj: a.parse_obj(s)?,
                shift: opt_int(input, "shift")?.unwrap_or(0),
            }])
        }
        (None, Some(_)) => summands_of(a, input, "summands"),
        _ => Err(Error::invalid(
            "give either 'object' (with an optional 'shift') or 'summands', the projective to \
             take the head of",
        )),
    }
}

fn module_head<A: Additive>(a: &A, input: &Value) -> Result<Value> {
    let ss = head_summands(a, input)?;
    let h = cm::radical_quotient(a, &ss, opt_int(input, "max_degree")?, DEFAULT_SPAN)?;
    Ok(with_prov(
        json!({
            "module": cm::module_json(a, &h.module)?,
            "dimensions": cm::dims_json(&h.dims),
            "dimension": cm::dims_total(&h.dims),
            "window": window_json(&h.window),
            "complete": !h.window.is_some_and(|w| w.truncated),
        }),
        prov_notes(a, h.notes),
    ))
}

fn report_value<A: Additive>(
    a: &A,
    r: &cm::Resolution<A::Obj, A::Mor>,
    max: Option<i64>,
) -> Result<(Value, Vec<String>)> {
    let rep = cm::check_resolution(a, r, max, DEFAULT_SPAN)?;
    let mut v = rep.json(r.complex.conv.step());
    v["certified_by"] = json!("certificate");
    Ok((v, rep.notes))
}

fn resolution_start<A: Additive>(a: &A, input: &Value) -> Result<Value> {
    let (m, mut notes) = cm::parse_module_noted(a, get(input, "module")?)?;
    let fs = cm::check_module(a, &m);
    if let Some(f) = fs.first() {
        return Err(Error::invalid(format!(
            "the module is not well formed: {}: {} (run module.check for all problems)",
            f.location, f.detail
        )));
    }
    let conv = complex::Conventions::parse(input)?;
    let r = cm::start_resolution(a, m, conv);
    let (check, cnotes) = report_value(a, &r, opt_int(input, "max_degree")?)?;
    notes.extend(cnotes);
    notes.push(
        "C₀ is the sum of the generators, mapping to the module by the identity; \
         resolution.extend covers the kernel of C₀ → M next"
            .into(),
    );
    Ok(with_prov(
        json!({ "resolution": cm::resolution_json(a, &r)?, "check": check }),
        prov_notes(a, notes),
    ))
}

fn resolution_extend<A: Additive>(a: &A, input: &Value) -> Result<Value> {
    let (mut r, mut notes) = cm::parse_resolution(a, get(input, "resolution")?)?;
    let max = opt_int(input, "max_degree")?;
    let before = cm::check_resolution(a, &r, max, DEFAULT_SPAN)?;
    if let Some(f) = before.failures.first() {
        return Err(Error::invalid(format!(
            "the resolution does not check ({} failure{}): {}: {} ({}); extending it would \
             build on that, so fix it first (resolution.check lists them all)",
            before.failures.len(),
            if before.failures.len() == 1 { "" } else { "s" },
            f.location,
            f.detail,
            f.condition
        )));
    }
    let e = cm::extend_resolution(a, &mut r, max, DEFAULT_SPAN)?;
    notes.push(
        "the new summands generate the kernel degree by degree, modulo what earlier ones \
         produce; then a summand whose generator lies in the submodule generated by the others \
         is dropped, so none is redundant"
            .into(),
    );
    notes.extend(complex::dropped_note(e.dropped, "the kernel"));
    if let Some(n) = window_note(&e.window, "the kernel") {
        notes.push(n);
    }
    let (check, cnotes) = report_value(a, &r, max)?;
    notes.extend(cnotes);
    Ok(with_prov(
        json!({
            "resolution": cm::resolution_json(a, &r)?,
            "added": {
                "at": pos_json(&e.at),
                "summands": complex::summands_json(a, &e.summands),
                "matrix": complex::matrix_json(a, &e.matrix)?,
            },
            "window": window_json(&e.window),
            "complete": !e.window.is_some_and(|w| w.truncated),
            "check": check,
        }),
        prov_notes(a, notes),
    ))
}

fn resolution_check<A: Additive>(a: &A, input: &Value) -> Result<Value> {
    let (r, mut notes) = cm::parse_resolution(a, get(input, "resolution")?)?;
    let (mut v, cnotes) = report_value(a, &r, opt_int(input, "max_degree")?)?;
    notes.extend(cnotes);
    v["resolution"] = cm::resolution_json(a, &r)?;
    v["provenance"] = prov_notes(a, notes);
    Ok(v)
}

/// How many basis vectors of the freedom at one order `complex.deform` lists.
const SHOWN_FREEDOM: usize = 20;

/// The unknown entries: references `{"map": "differential", "from": p}` to differentials of the
/// complex, with `row` and `col` to pick one entry (either alone picks a row or a column).
fn parse_unknowns<O: Clone, M: Clone>(c: &Complex<O, M>, v: Option<&Value>) -> Result<Mask> {
    let mut mask = Mask::new();
    let Some(v) = v.filter(|v| !v.is_null()) else {
        return Ok(mask);
    };
    let items = v
        .as_array()
        .ok_or_else(|| Error::invalid("'unknowns' is a list of references to differentials"))?;
    for (k, r) in items.iter().enumerate() {
        crate::keys::check(
            r,
            &["map", "from", "axis", "row", "col"],
            &format!("unknown {k}"),
        )?;
        if r.get("map").and_then(Value::as_str) != Some("differential") {
            return Err(Error::invalid(format!(
                "unknown {k}: give {{\"map\": \"differential\", \"from\": p}}, with \"row\" \
                 and \"col\" for one entry"
            )));
        }
        if opt_uint(r, "axis", 0)? != 0 {
            return Err(Error::invalid(format!(
                "unknown {k}: deformations are solved for one-dimensional complexes (axis 0)"
            )));
        }
        let p = complex::position(get(r, "from")?, c.dim)?;
        let (rows, cols) = (c.at(&c.next(&p, 0)).len(), c.at(&p).len());
        let pick = |key: &str, n: usize| -> Result<Vec<usize>> {
            match r.get(key) {
                None | Some(Value::Null) => Ok((0..n).collect()),
                Some(x) => x.as_u64().map(|i| vec![i as usize]).ok_or_else(|| {
                    Error::invalid(format!("unknown {k}: '{key}' is a nonnegative integer"))
                }),
            }
        };
        let ents = mask.entry(p).or_default();
        for i in pick("row", rows)? {
            for j in pick("col", cols)? {
                ents.insert((i, j));
            }
        }
    }
    Ok(mask)
}

fn deform_op<A: ParamSplit>(t: &Truncated<'_, A>, input: &Value) -> Result<Value> {
    t.linear_algebra()?;
    let c = parse_complex(t, get(input, "complex")?)?;
    let mask = parse_unknowns(&c, input.get("unknowns"))?;
    let d = cd::deform(t, &c, &mask)?;
    let order_zero_fails = matches!(d.obstruction, Some((0, _)));
    let solved_to = if order_zero_fails {
        Value::Null
    } else {
        json!(d.orders.last().map(|o| o.order).unwrap_or(0))
    };
    let mut orders = Vec::new();
    for o in &d.orders {
        let freedom = o
            .freedom
            .iter()
            .take(SHOWN_FREEDOM)
            .map(|f| map_json(t, &cd::lift_map(t, o.order, f)?))
            .collect::<Result<Vec<_>>>()?;
        orders.push(json!({
            "order": o.order,
            "correction": map_json(t, &cd::lift_map(t, o.order, &o.correction)?)?,
            "freedom_dimension": o.freedom.len(),
            "freedom": freedom,
        }));
    }
    let failures: Vec<Value> = d
        .obstruction
        .iter()
        .map(|(m, f)| {
            let mut v = f.json();
            v["order"] = json!(m);
            v
        })
        .collect();
    let complex = cd::assemble(t, &c, &d.parts)?;
    let mut notes = vec![format!(
        "the unknown entries were corrected by {h}^m X_m, 1 ≤ m ≤ {n}, solving d ∘ d = 0 one \
         order of {h} at a time; each order's correction is one choice: any other differs from \
         it by a combination of that order's freedom (choices at one order change the equations \
         at later orders)",
        h = t.param,
        n = t.order
    )];
    for o in &d.orders {
        if o.freedom.len() > SHOWN_FREEDOM {
            notes.push(format!(
                "order {}: the freedom has dimension {}; only the first {SHOWN_FREEDOM} vectors \
                 of its basis are listed",
                o.order,
                o.freedom.len()
            ));
        }
    }
    if let Some((m, _)) = &d.obstruction {
        notes.push(format!(
            "obstructed at order {m}: the complex returned is corrected through order {}",
            m.saturating_sub(1)
        ));
    }
    Ok(with_prov(
        json!({
            "solved": d.obstruction.is_none(),
            "solved_to_order": solved_to,
            "complex": complex_json(t, &complex)?,
            "orders": orders,
            "failures": failures,
        }),
        prov_notes(t, notes),
    ))
}
