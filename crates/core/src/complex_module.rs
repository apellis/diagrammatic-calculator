//! Modules given by projective presentations, and projective resolutions of them
//! (`docs/complexes.md`, "Modules" and "Resolutions").
//!
//! A module is `M = coker(φ : Q₁ → Q₀)` for a matrix `φ` between sums of shifted objects. Since
//! the objects are projective, `Hom(T⟨d⟩, M) = Hom(T⟨d⟩, Q₀) / φ ∘ Hom(T⟨d⟩, Q₁)`, so every
//! question about `M` is linear algebra at the test objects. A resolution of `M` is a complex
//! `… → C₋₁ → C₀` (cohomological indexing; `C₀` is the term at position 0) with an augmentation
//! `ε : C₀ → Q₀` inducing `C₀ → M`. The module stays fixed while the resolution changes: Gaussian
//! elimination and new kernel steps carry the augmentation along.

use crate::complex::{
    self, Additive, Complex, Failure, Generator, Matrix, Noted, Pos, Space, Summand, Window,
    check_entries, check_zero, complex_json, fmt_pos, homology_piece, kernel_generators, mat_mul,
    matrix_json, parse_complex_noted, parse_matrix, postcompose, range_over_tests,
    relative_kernel_generators, space, subspace_generators, summands_json, window,
};
use crate::error::{Error, Result};
use crate::linalg::Mat;
use crate::ring::Ring;
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// Linear conditions per test object: rows of a matrix and their width.
type Conditions<A> = BTreeMap<String, (Vec<Vec<E<A>>>, usize)>;

/// A reduced resolution, the eliminations performed, and whether the limit stopped them.
pub type GaussResolution<A> = (
    Resolution<<A as Additive>::Obj, <A as Additive>::Mor>,
    Vec<crate::complex_build::Elimination>,
    bool,
);

type E<A> = <<A as Additive>::Field as Ring>::Elem;

/// `coker(φ : Q₁ → Q₀)`: `matrix` has a row for each generator and a column for each relation.
pub struct Module<O, M> {
    pub gens: Vec<Summand<O>>,
    pub rels: Vec<Summand<O>>,
    pub matrix: Matrix<M>,
}

/// Rewrites the locations of the presentation read as a two-term complex.
fn relabel(s: &str) -> String {
    s.replace("the differential from -1", "the presentation matrix")
        .replace("the term at -1", "the relations")
        .replace("the term at 0", "the generators")
        .replace("⟩ at -1", "⟩ among the relations")
        .replace("⟩ at 0", "⟩ among the generators")
}

/// Parses a module `{generators, relations, matrix}` (`spec/ops/module.check.schema.json`).
/// Objects and shifts may be left out where the matrix entries determine them, as for complexes.
pub fn parse_module_noted<A: Additive>(a: &A, v: &Value) -> Result<Noted<Module<A::Obj, A::Mor>>> {
    if v.is_string() {
        return Err(Error::invalid(format!(
            "the module {v} is a name: give the module itself, {{\"generators\": […], \
             \"relations\": […], \"matrix\": […]}} (names of module cells are understood \
             only inside a workspace)"
        )));
    }
    let gens = v
        .get("generators")
        .filter(|g| !g.is_null())
        .ok_or_else(|| {
            Error::invalid(
                "a module needs 'generators', a list of summands {object, shift}, and may have \
             'relations' and 'matrix' (a row for each generator, a column for each relation)",
            )
        })?;
    let rels = v.get("relations").filter(|r| !r.is_null());
    let has_rels = rels
        .and_then(Value::as_array)
        .is_some_and(|r| !r.is_empty());
    let mut terms = vec![json!({ "at": 0, "summands": gens })];
    let mut diffs = vec![];
    if has_rels {
        terms.insert(0, json!({ "at": -1, "summands": rels }));
        let m = v.get("matrix").filter(|m| !m.is_null()).ok_or_else(|| {
            Error::invalid(
                "a module with relations needs 'matrix': a row for each generator and a column \
                 for each relation, entry (i, j) the component of relation j at generator i",
            )
        })?;
        diffs.push(json!({ "from": -1, "matrix": m }));
    } else if let Some(r) = rels
        && !r.is_array()
    {
        return Err(Error::invalid("'relations' is a list of summands"));
    }
    let cv = json!({ "terms": terms, "differentials": diffs });
    let (c, notes) = parse_complex_noted(a, &cv).map_err(|e| Error {
        message: relabel(&e.message),
        ..e
    })?;
    let gens = c.at(&[0]).to_vec();
    let rels = c.at(&[-1]).to_vec();
    let matrix = c.diff(a, 0, &[-1]);
    let notes = notes
        .into_iter()
        .map(|n| relabel(&n).replace("differential entries", "matrix entries"))
        .collect();
    Ok((Module { gens, rels, matrix }, notes))
}

pub fn module_json<A: Additive>(a: &A, m: &Module<A::Obj, A::Mor>) -> Result<Value> {
    Ok(json!({
        "generators": summands_json(a, &m.gens),
        "relations": summands_json(a, &m.rels),
        "matrix": matrix_json(a, &m.matrix)?,
    }))
}

/// Checks that each matrix entry is a degree-zero map from its relation to its generator.
pub fn check_module<A: Additive>(a: &A, m: &Module<A::Obj, A::Mor>) -> Vec<Failure> {
    let mut fs = Vec::new();
    check_entries(
        a,
        &m.rels,
        &m.gens,
        &m.matrix,
        0,
        "the presentation matrix",
        &mut fs,
    );
    fs
}

fn rank<K: Ring>(k: &K, m: &Mat<K::Elem>) -> Result<usize> {
    if m.nrows() == 0 || m.ncols() == 0 {
        return Ok(0);
    }
    crate::linalg::rank(k, m)
}

/// A basis of the kernel of `m` (`n` columns), also when `m` has no rows.
fn kernel_of<K: Ring>(k: &K, n: usize, m: &Mat<K::Elem>) -> Result<Vec<Vec<K::Elem>>> {
    if m.nrows() == 0 {
        return Ok((0..n)
            .map(|i| {
                let mut v = vec![k.zero(); n];
                v[i] = k.one();
                v
            })
            .collect());
    }
    if n == 0 {
        return Ok(vec![]);
    }
    crate::linalg::kernel(k, m)
}

/// Graded dimensions at the test objects: test ↦ degree ↦ dimension (zero ones left out).
pub type Dims = BTreeMap<String, BTreeMap<i64, i64>>;

pub fn dims_json(d: &Dims) -> Value {
    Value::Array(
        d.iter()
            .map(|(t, m)| {
                json!({
                    "test": t,
                    "dimensions": crate::complex_build::laurent_text(m),
                    "powers": m.iter().map(|(e, k)| json!([e, k])).collect::<Vec<_>>(),
                })
            })
            .collect(),
    )
}

pub fn dims_total(d: &Dims) -> i64 {
    d.values().flat_map(|m| m.values()).sum()
}

fn add_dim(d: &mut Dims, t: String, deg: i64, n: usize) {
    if n > 0 {
        *d.entry(t).or_default().entry(deg).or_default() += n as i64;
    }
}

/// `dim Hom(T⟨d⟩, M)` for `M = coker φ`, in each degree of the window.
pub fn module_dims<A: Additive>(
    a: &A,
    m: &Module<A::Obj, A::Mor>,
    tests: &[A::Obj],
    max: Option<i64>,
    span: i64,
) -> Result<(Dims, Option<Window>)> {
    let k = a.field().clone();
    let w = window(range_over_tests(a, tests, &m.gens)?, max, span);
    let mut out = Dims::new();
    if let Some(w) = w {
        for d in w.lo..=w.hi {
            for t in tests {
                let sq = space(a, t, d, &m.gens)?;
                if sq.basis.is_empty() {
                    continue;
                }
                let sr = space(a, t, d, &m.rels)?;
                let r = if sr.basis.is_empty() {
                    0
                } else {
                    rank(&k, &postcompose(a, t, d, &m.gens, &m.matrix, &sr, &sq)?)?
                };
                add_dim(&mut out, a.fmt_obj(t), d, sq.basis.len() - r);
            }
        }
    }
    Ok((out, w))
}

/// The objects of a module, for choosing test objects.
pub fn module_objects<O: Clone, M>(m: &Module<O, M>) -> Vec<O> {
    m.gens
        .iter()
        .chain(&m.rels)
        .map(|s| s.obj.clone())
        .collect()
}

/// The radical of `E = End⁰(X)`: the radical of its trace form `(u, v) ↦ tr(w ↦ u v w)`, valid
/// in characteristic 0 or larger than `dim E`. Returns a basis of the linear conditions
/// cutting `J(E)` out of `E` (in the coordinates of `hom_basis(X, X, 0)`) and `dim E`.
fn jacobson_conditions<A: Additive>(a: &A, x: &A::Obj) -> Result<(Vec<Vec<E<A>>>, usize)> {
    let k = a.field().clone();
    let b = a.hom_basis(x, x, 0)?;
    let n = b.len();
    let p = k.characteristic();
    if p != 0 && (p as usize) <= n {
        return Err(Error::precondition(format!(
            "the radical of {} is computed with a trace form, which needs characteristic 0 or \
             larger than dim End⁰ = {n}; the ring here has characteristic {p}",
            a.fmt_obj(x)
        )));
    }
    // tr(L_u) for each basis element u
    let mut tr = Vec::with_capacity(n);
    for u in &b {
        let mut t = k.zero();
        for (j, w) in b.iter().enumerate() {
            let c = a.coords(x, x, 0, &a.compose(u, w))?;
            t = k.add(&t, &c[j]);
        }
        tr.push(t);
    }
    // the form T_ij = tr(L_{b_i b_j}), by linearity of tr(L_·) in the coordinates of b_i b_j
    let mut rows = Vec::with_capacity(n);
    for j in 0..n {
        let mut row = Vec::with_capacity(n);
        for bi in &b {
            let c = a.coords(x, x, 0, &a.compose(bi, &b[j]))?;
            let mut s = k.zero();
            for (l, cl) in c.iter().enumerate() {
                s = k.add(&s, &k.mul(cl, &tr[l]));
            }
            row.push(s);
        }
        rows.push(row);
    }
    if n == 0 {
        return Ok((vec![], 0));
    }
    let m = Mat::from_rows(n, rows)?;
    Ok((crate::linalg::row_space(&k, &m)?, n))
}

/// `rad(T⟨d⟩, ⊕ X_i⟨s_i⟩)` as vectors in [`space`]: the maps `f` with `f ∘ g ∈ J(End⁰ X_i)` for
/// every `g : X_i⟨s_i⟩ → T⟨d⟩`, summand by summand.
fn radical_piece<A: Additive>(
    a: &A,
    t: &A::Obj,
    d: i64,
    ss: &[Summand<A::Obj>],
    sp: &Space<A::Mor>,
    conds: &mut Conditions<A>,
) -> Result<Vec<Vec<E<A>>>> {
    let k = a.field().clone();
    let n = sp.basis.len();
    let mut out = Vec::new();
    for (i, s) in ss.iter().enumerate() {
        let (off, dim) = (sp.offsets[i], sp.dims[i]);
        if dim == 0 {
            continue;
        }
        let key = a.fmt_obj(&s.obj);
        if !conds.contains_key(&key) {
            conds.insert(key.clone(), jacobson_conditions(a, &s.obj)?);
        }
        let (rows, _) = &conds[&key];
        let back = match a.hom_degrees(&s.obj, t)? {
            Some((lo, hi)) if s.shift - d >= lo && hi.is_none_or(|h| s.shift - d <= h) => {
                a.hom_basis(&s.obj, t, s.shift - d)?
            }
            _ => vec![],
        };
        let fs: Vec<&A::Mor> = sp.basis[off..off + dim].iter().map(|(_, f)| f).collect();
        let mut cond_rows: Vec<Vec<E<A>>> = Vec::new();
        for g in &back {
            let us: Vec<Vec<E<A>>> = fs
                .iter()
                .map(|f| a.coords(&s.obj, &s.obj, 0, &a.compose(f, g)))
                .collect::<Result<_>>()?;
            for r in rows {
                cond_rows.push(
                    us.iter()
                        .map(|u| {
                            r.iter()
                                .zip(u)
                                .fold(k.zero(), |acc, (x, y)| k.add(&acc, &k.mul(x, y)))
                        })
                        .collect(),
                );
            }
        }
        let c = if cond_rows.is_empty() {
            Mat::from_rows(dim, vec![])?
        } else {
            Mat::from_rows(dim, cond_rows)?
        };
        for z in kernel_of(&k, dim, &c)? {
            let mut v = vec![k.zero(); n];
            v[off..off + dim].clone_from_slice(&z);
            out.push(v);
        }
    }
    Ok(out)
}

/// The quotient `X / rad X` of a sum of shifted objects, presented by generators of the radical,
/// with its graded dimensions and notes on how it was found.
pub struct Head<O, M> {
    pub module: Module<O, M>,
    pub dims: Dims,
    pub window: Option<Window>,
    pub notes: Vec<String>,
}

pub fn radical_quotient<A: Additive>(
    a: &A,
    ss: &[Summand<A::Obj>],
    max: Option<i64>,
    span: i64,
) -> Result<Head<A::Obj, A::Mor>> {
    a.linear_algebra()?;
    let objs: Vec<A::Obj> = ss.iter().map(|s| s.obj.clone()).collect();
    let tests = a.test_objects(&objs)?;
    let w = window(range_over_tests(a, &tests, ss)?, max, span);
    let mut conds = BTreeMap::new();
    let mut gens: Vec<Generator<A::Obj, A::Mor>> = Vec::new();
    let mut dims = Dims::new();
    if let Some(w) = w {
        gens = subspace_generators(a, ss, &tests, w, |t, d, sp| {
            let r = radical_piece(a, t, d, ss, sp, &mut conds)?;
            add_dim(&mut dims, a.fmt_obj(t), d, sp.basis.len() - r.len());
            Ok(r)
        })?;
    }
    let rels: Vec<Summand<A::Obj>> = gens
        .iter()
        .map(|g| Summand {
            obj: g.obj.clone(),
            shift: g.degree,
        })
        .collect();
    let matrix = (0..ss.len())
        .map(|i| gens.iter().map(|g| g.column[i].clone()).collect())
        .collect();
    let mut notes = vec![
        "rad(T⟨d⟩, X) is the set of maps f with f ∘ g in the Jacobson radical J of End⁰(X) for \
         every map g back; J is the radical of the trace form of End⁰(X)"
            .to_string(),
        "the relations generate the radical degree by degree, modulo what earlier relations \
         produce"
            .to_string(),
    ];
    for (o, (rows, n)) in &conds {
        let q = n - rows.len();
        if q > 1 {
            notes.push(format!(
                "End⁰({o})/J has dimension {q}, so {o}/rad may not be simple (it is simple \
                 exactly when End⁰({o})/J is a division algebra; it is not, for instance, when \
                 {o} is a sum of several indecomposables)"
            ));
        }
    }
    if let Some(w) = w
        && w.truncated
    {
        notes.push(format!(
            "the radical was searched for relations in degrees {} to {} only: the Hom spaces \
             are not bounded above there (raise max_degree to search further); the dimensions \
             of the quotient are exact in every degree listed",
            w.lo, w.hi
        ));
    }
    Ok(Head {
        module: Module {
            gens: ss.to_vec(),
            rels,
            matrix,
        },
        dims,
        window: w,
        notes,
    })
}

/// A complex `… → C₋₁ → C₀` with an augmentation `ε : C₀ → Q₀` onto the module `coker φ`.
pub struct Resolution<O, M> {
    pub complex: Complex<O, M>,
    pub module: Module<O, M>,
    pub aug: Matrix<M>,
}

/// The position of `C₀`.
const ZERO: [i64; 1] = [0];

/// Parses a complex with `resolves: {module, augmentation}`. The augmentation may be left out
/// when `C₀` is the sum of the generators of the module, in order: it is then the identity.
pub fn parse_resolution<A: Additive>(
    a: &A,
    v: &Value,
) -> Result<Noted<Resolution<A::Obj, A::Mor>>> {
    let r = v.get("resolves").filter(|r| !r.is_null()).ok_or_else(|| {
        Error::invalid(
            "a resolution is a complex with 'resolves': {\"module\": …, \"augmentation\": …}; \
             start one with resolution.start",
        )
    })?;
    let (c, mut notes) = parse_complex_noted(a, v)?;
    if c.dim != 1 {
        return Err(Error::invalid("a resolution is a one-dimensional complex"));
    }
    let step = c.conv.step();
    if let Some((p, _)) = c
        .terms
        .iter()
        .find(|(p, ss)| p[0] * step > 0 && !ss.is_empty())
    {
        return Err(Error::invalid(format!(
            "a resolution ends at position 0, the term mapping to the module, but this one has \
             a term at {}",
            fmt_pos(p)
        )));
    }
    let (module, mnotes) = parse_module_noted(
        a,
        r.get("module")
            .ok_or_else(|| Error::invalid("'resolves' needs 'module'"))?,
    )?;
    notes.extend(mnotes.into_iter().map(|n| format!("in the module: {n}")));
    let c0 = c.at(&ZERO).to_vec();
    let aug = match r.get("augmentation").filter(|x| !x.is_null()) {
        Some(m) => parse_matrix(a, m, module.gens.len(), c0.len(), "the augmentation")?,
        None => {
            let same = c0.len() == module.gens.len()
                && c0
                    .iter()
                    .zip(&module.gens)
                    .all(|(x, y)| a.key(&x.obj) == a.key(&y.obj) && x.shift == y.shift);
            if !same {
                return Err(Error::invalid(
                    "the resolution needs 'augmentation', the matrix of C₀ → Q₀ (a row for each \
                     generator of the module, a column for each summand of C₀); it may be left \
                     out only when C₀ is the sum of the generators, in order",
                ));
            }
            (0..c0.len())
                .map(|i| {
                    (0..c0.len())
                        .map(|j| {
                            if i == j {
                                a.identity(&c0[i].obj)
                            } else {
                                a.zero()
                            }
                        })
                        .collect()
                })
                .collect()
        }
    };
    Ok((
        Resolution {
            complex: c,
            module,
            aug,
        },
        notes,
    ))
}

pub fn resolution_json<A: Additive>(a: &A, r: &Resolution<A::Obj, A::Mor>) -> Result<Value> {
    let mut v = complex_json(a, &r.complex)?;
    v["resolves"] = json!({
        "module": module_json(a, &r.module)?,
        "augmentation": matrix_json(a, &r.aug)?,
    });
    Ok(v)
}

/// `C₀` alone, with the identity onto the generators.
pub fn start_resolution<A: Additive>(
    a: &A,
    m: Module<A::Obj, A::Mor>,
    conv: complex::Conventions,
) -> Resolution<A::Obj, A::Mor> {
    let n = m.gens.len();
    let aug = (0..n)
        .map(|i| {
            (0..n)
                .map(|j| {
                    if i == j {
                        a.identity(&m.gens[i].obj)
                    } else {
                        a.zero()
                    }
                })
                .collect()
        })
        .collect();
    let mut terms = BTreeMap::new();
    terms.insert(ZERO.to_vec(), m.gens.clone());
    Resolution {
        complex: Complex {
            dim: 1,
            conv,
            terms,
            diffs: BTreeMap::new(),
        },
        module: m,
        aug,
    }
}

/// The far end of the resolution: the position furthest from 0 with a term (0 if none).
fn start_of<O, M>(c: &Complex<O, M>) -> i64 {
    let step = c.conv.step();
    c.terms
        .iter()
        .filter(|(_, ss)| !ss.is_empty())
        .map(|(p, _)| p[0])
        .min_by_key(|p| p * step)
        .unwrap_or(0)
}

/// The result of checking a resolution.
pub struct ResolutionReport {
    pub failures: Vec<Failure>,
    /// The far end, where the kernel still to be covered lives, with its dimensions.
    pub start: i64,
    pub open: Dims,
    pub module: Dims,
    pub complete: bool,
    pub minimal: Option<bool>,
    pub notes: Vec<String>,
}

impl ResolutionReport {
    pub fn resolved(&self) -> bool {
        self.failures.is_empty() && self.open.is_empty() && self.complete
    }

    pub fn json(&self, step: i64) -> Value {
        let open = !self.open.is_empty();
        json!({
            "holds": self.failures.is_empty(),
            "failures": self.failures.iter().take(20).map(Failure::json).collect::<Vec<_>>(),
            "failure_count": self.failures.len(),
            "resolved": self.resolved(),
            "complete": self.complete,
            "length": self.start.abs(),
            "next_at": if open { json!(self.start - step) } else { Value::Null },
            "uncovered": dims_json(&self.open),
            "uncovered_dimension": dims_total(&self.open),
            "module": dims_json(&self.module),
            "module_dimension": dims_total(&self.module),
            "minimal": self.minimal,
        })
    }
}

/// Checks a resolution: the complex, the entries of `ε`, that `ε ∘ d` lands in the relations
/// (so `ε` induces `C₀ → M` and makes `C₋₁ → C₀ → M → 0` a complex), that `C₀ → M` is onto,
/// and exactness at every position but the far end, where the kernel still to be covered is
/// reported. All at the test objects, in the degree windows.
pub fn check_resolution<A: Additive>(
    a: &A,
    r: &Resolution<A::Obj, A::Mor>,
    max: Option<i64>,
    span: i64,
) -> Result<ResolutionReport> {
    a.linear_algebra()?;
    let k = a.field().clone();
    let c = &r.complex;
    let m = &r.module;
    let mut failures = complex::check_complex(a, c)?;
    for f in check_module(a, m) {
        failures.push(Failure {
            location: format!("the module: {}", f.location),
            ..f
        });
    }
    let c0 = c.at(&ZERO).to_vec();
    check_entries(
        a,
        &c0,
        &m.gens,
        &r.aug,
        0,
        "the augmentation",
        &mut failures,
    );
    let start = start_of(c);
    let report = |failures, open, module, complete, notes| ResolutionReport {
        failures,
        start,
        open,
        module,
        complete,
        minimal: None,
        notes,
    };
    if !failures.is_empty() {
        return Ok(report(failures, Dims::new(), Dims::new(), false, vec![]));
    }
    let prev0 = c.prev(&ZERO, 0);
    let cm = c.at(&prev0).to_vec();
    let d0 = c.diff(a, 0, &prev0);
    if !cm.is_empty() {
        let ed = mat_mul(a, &r.aug, &d0, m.gens.len(), cm.len());
        let lands = if m.rels.is_empty() {
            let mut fs = Vec::new();
            check_zero(a, &ed, "augmentation", "ε ∘ d", &mut fs)?;
            fs.is_empty()
        } else {
            crate::complex_solve::lift(a, &cm, &m.rels, &m.gens, &ed, &m.matrix)?.is_some()
        };
        if !lands {
            failures.push(Failure {
                condition: "augmentation",
                location: format!("ε ∘ d from {}", fmt_pos(&prev0)),
                detail: if m.rels.is_empty() {
                    "ε ∘ d is not zero, so C₋₁ → C₀ → M is not a complex".into()
                } else {
                    "ε ∘ d does not factor through the presentation matrix, so its image in M \
                     is not zero and C₋₁ → C₀ → M is not a complex"
                        .into()
                },
            });
            return Ok(report(failures, Dims::new(), Dims::new(), false, vec![]));
        }
    }
    let objs: Vec<A::Obj> = c
        .terms
        .values()
        .flatten()
        .map(|s| s.obj.clone())
        .chain(module_objects(m))
        .collect();
    let tests = a.test_objects(&objs)?;
    let mut complete = true;
    let mut notes = vec![format!(
        "checked at the test objects {}",
        tests
            .iter()
            .map(|t| a.fmt_obj(t))
            .collect::<Vec<_>>()
            .join(", ")
    )];
    let mut module = Dims::new();
    let mut open = Dims::new();
    // onto and exactness at C₀
    let both: Vec<Summand<A::Obj>> = c0.iter().chain(&m.gens).cloned().collect();
    let w0 = window(range_over_tests(a, &tests, &both)?, max, span);
    if let Some(w) = w0 {
        complete &= !w.truncated;
        for d in w.lo..=w.hi {
            for t in &tests {
                let (s0, sq) = (space(a, t, d, &c0)?, space(a, t, d, &m.gens)?);
                let sr = space(a, t, d, &m.rels)?;
                let ea = postcompose(a, t, d, &m.gens, &r.aug, &s0, &sq)?;
                let pb = postcompose(a, t, d, &m.gens, &m.matrix, &sr, &sq)?;
                let rb = rank(&k, &pb)?;
                let rab = if sr.basis.is_empty() {
                    rank(&k, &ea)?
                } else if s0.basis.is_empty() {
                    rb
                } else {
                    rank(&k, &ea.hstack(&pb)?)?
                };
                let tn = a.fmt_obj(t);
                add_dim(&mut module, tn.clone(), d, sq.basis.len() - rb);
                if rab < sq.basis.len() {
                    failures.push(Failure {
                        condition: "onto",
                        location: format!("the augmentation at {tn}⟨{d}⟩"),
                        detail: format!(
                            "C₀ → M is not onto: its image misses {} of the {} dimensions of \
                             Hom({tn}⟨{d}⟩, M)",
                            sq.basis.len() - rab,
                            sq.basis.len() - rb
                        ),
                    });
                }
                let dim_k = s0.basis.len() - (rab - rb);
                let r_in = if cm.is_empty() {
                    0
                } else {
                    let sm = space(a, t, d, &cm)?;
                    rank(&k, &postcompose(a, t, d, &c0, &d0, &sm, &s0)?)?
                };
                let h = dim_k - r_in;
                if h > 0 {
                    if start == 0 {
                        add_dim(&mut open, tn, d, h);
                    } else {
                        failures.push(Failure {
                            condition: "exact",
                            location: format!("position 0 at {tn}⟨{d}⟩"),
                            detail: format!(
                                "the kernel of C₀ → M is not the image of C₋₁: {h} \
                                 dimension{} left over",
                                if h == 1 { "" } else { "s" }
                            ),
                        });
                    }
                }
            }
        }
    }
    // exactness at the other positions
    for p in c.positions() {
        if p == ZERO {
            continue;
        }
        let here = c.at(&p);
        let w = window(range_over_tests(a, &tests, here)?, max, span);
        let Some(w) = w else { continue };
        complete &= !w.truncated;
        for d in w.lo..=w.hi {
            for t in &tests {
                let h = homology_piece(a, c, &p, 0, t, d, 0)?.dim;
                if h == 0 {
                    continue;
                }
                let tn = a.fmt_obj(t);
                if p[0] == start {
                    add_dim(&mut open, tn, d, h);
                } else {
                    failures.push(Failure {
                        condition: "exact",
                        location: format!("position {} at {tn}⟨{d}⟩", fmt_pos(&p)),
                        detail: format!("homology of dimension {h}"),
                    });
                }
            }
        }
    }
    if !complete {
        notes.push(format!(
            "some Hom spaces are not bounded above, so only finitely many degrees were checked \
             (up to {} above the lowest unless max_degree is given)",
            span
        ));
    }
    let mut out = report(failures, open, module, complete, notes);
    if out.resolved() {
        let minimal = crate::complex_solve::check_minimal(a, c)?.is_empty();
        out.minimal = Some(minimal);
    }
    Ok(out)
}

/// What [`extend_resolution`] added.
pub struct Extension<O, M> {
    pub at: Pos,
    pub summands: Vec<Summand<O>>,
    pub matrix: Matrix<M>,
    pub window: Option<Window>,
}

/// Covers the kernel at the far end: of `C₀ → M` when the resolution is `C₀` alone, else of the
/// last differential. Adds the generators found as the next term.
pub fn extend_resolution<A: Additive>(
    a: &A,
    r: &mut Resolution<A::Obj, A::Mor>,
    max: Option<i64>,
    span: i64,
) -> Result<Extension<A::Obj, A::Mor>> {
    a.linear_algebra()?;
    let c = &r.complex;
    let start = start_of(c);
    let s = vec![start];
    let src = c.at(&s).to_vec();
    if src.is_empty() {
        return Err(Error::invalid(
            "the resolution has no term at 0: the module is zero, or C₀ is missing",
        ));
    }
    let objs: Vec<A::Obj> = c
        .terms
        .values()
        .flatten()
        .map(|s| s.obj.clone())
        .chain(module_objects(&r.module))
        .collect();
    let tests = a.test_objects(&objs)?;
    let (gens, w) = if start == 0 {
        relative_kernel_generators(
            a,
            &src,
            &r.module.gens,
            &r.aug,
            Some((&r.module.rels, &r.module.matrix)),
            &tests,
            max,
            span,
        )?
    } else {
        let q = c.next(&s, 0);
        kernel_generators(a, &src, c.at(&q), &c.diff(a, 0, &s), &tests, max, span)?
    };
    let summands: Vec<Summand<A::Obj>> = gens
        .iter()
        .map(|g| Summand {
            obj: g.obj.clone(),
            shift: g.degree,
        })
        .collect();
    let matrix: Matrix<A::Mor> = (0..src.len())
        .map(|i| gens.iter().map(|g| g.column[i].clone()).collect())
        .collect();
    let at = r.complex.prev(&s, 0);
    if !summands.is_empty() {
        r.complex.terms.insert(at.clone(), summands.clone());
        r.complex.diffs.insert((0, at.clone()), matrix.clone());
    }
    Ok(Extension {
        at,
        summands,
        matrix,
        window: w,
    })
}

/// Gaussian elimination on a resolution, keeping the module: when an eliminated pair has its
/// target in `C₀`, that summand's column of `ε` is dropped. (With `d = [[φ, δ], [γ, ε']]` from
/// `X ⊕ A` to `Y ⊕ B = C₀`, the new `C₀` is `B` with the restriction of `ε`; it still induces
/// the same map to `M` up to the homotopy equivalence, since `ε_Y φ + ε_B γ` maps into the
/// relations.)
pub fn gauss_resolution<A: Additive>(
    a: &A,
    r: &Resolution<A::Obj, A::Mor>,
    limit: usize,
) -> Result<GaussResolution<A>> {
    let (c, steps, finished) = crate::complex_build::gauss(a, &r.complex, limit)?;
    let mut aug = r.aug.clone();
    let step = r.complex.conv.step();
    for e in &steps {
        if e.from + step == 0 {
            for row in aug.iter_mut() {
                row.remove(e.target);
            }
        }
    }
    let module = Module {
        gens: r.module.gens.clone(),
        rels: r.module.rels.clone(),
        matrix: r.module.matrix.clone(),
    };
    Ok((
        Resolution {
            complex: c,
            module,
            aug,
        },
        steps,
        finished,
    ))
}
