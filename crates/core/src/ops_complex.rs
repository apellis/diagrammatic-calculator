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
use std::collections::BTreeMap;

/// The operations of this module.
pub const OPS: [&str; 18] = [
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
];

/// How many elimination steps `complex.gauss` takes at most when no `limit` is given.
pub const DEFAULT_GAUSS_LIMIT: usize = 1000;

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
    complex::check_entries(a, &x, &z, &f, 0, "'f'", &mut fs);
    if lifting {
        complex::check_entries(a, &y, &z, &g, 0, "'g'", &mut fs);
    } else {
        complex::check_entries(a, &x, &y, &g, 0, "'g'", &mut fs);
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
        complex::check_entries(a, c.at(p), d.at(p), m, 0, &loc, &mut fs);
        if let Some(e) = fs.first() {
            return Err(Error::invalid(format!("{}: {}", e.location, e.detail)));
        }
    }
    let (free, sol) = crate::complex_solve::solve_chain_map(a, &c, &d, &fixed)?;
    let mut v = json!({
        "free_positions": free.iter().map(|p| pos_json(p)).collect::<Vec<_>>(),
    });
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
            }
        }
    }
    Ok(with_prov(
        v,
        prov_notes(
            a,
            vec![
                "unknown components at every position where both complexes are nonzero and no \
                 component is fixed; d F = F d imposed along every axis; 'freedom' is the \
                 dimension of the chain maps vanishing on the fixed components"
                    .into(),
            ],
        ),
    ))
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
    let c = valid_complex(a, get(input, "complex")?, "complex")?;
    let limit = opt_uint(input, "limit", DEFAULT_GAUSS_LIMIT)?;
    let (r, steps, finished) = crate::complex_build::gauss(a, &c, limit)?;
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
            "complex": complex_json(a, &r)?,
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
    let classes = crate::complex_build::euler(a, &c);
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
    Ok(with_prov(
        json!({ "terms": terms, "text": text }),
        prov_notes(
            a,
            vec![
                "Σ_p (−1)^(p_0 + … + p_(n−1)) Σ q^s [X] over the summands X⟨s⟩ of C(p), with \
                 isotopic objects grouped; the classes [X] are not reduced further"
                    .into(),
            ],
        ),
    ))
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
    let (c, mut notes) = complex::parse_complex_noted(a, v)?;
    let cj = complex_json(a, &c)?;
    let fs = check_complex(a, &c)?;
    let mut out = json!({
        "complex": cj,
        "check": verdict(a, &fs, vec![]),
        "euler": euler(a, &json!({ "complex": cj }))?,
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
