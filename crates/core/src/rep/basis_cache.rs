//! Echelon forms of basis evaluations, kept across calls.
//!
//! The basis normal forms of the KL and BE2 2-categories (`crate::rep::kl`, `crate::rep::skm`)
//! evaluate every basis word of a hom space `HOM(s, t)` in a degree and bring those rows to
//! echelon form; a term of that hom space is then written in the basis by reducing its own
//! evaluation against the echelon form. The rows cost almost all of the time, and they depend only
//! on the context and the hom space, not on the term. This module keeps the echelon forms in a
//! bounded least-recently-used cache that outlives one evaluator (and so one call, one session, or
//! one cell of a document: in the browser, everything one worker computes).
//!
//! The key holds everything the rows depend on. The context enters as a fingerprint: the name of
//! the strategy and representation, the type of the ground ring and the complete debug form of the
//! [`Context`] (presentation, specification, colours, Cartan datum, weights, region weight `μ`,
//! parameters, scalars, ground ring with its run-time data, coefficient rings), compared in full,
//! not by a hash. The rest of the key is the source and target, the degree and the numbers of dot
//! and bubble variables of the evaluation ring. Results are the same with or without the cache.

use crate::context::Context;
use crate::error::{Error, Result};
use crate::ring::Ring;
use crate::term::Object;
use std::any::Any;
use std::cell::Cell;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

/// At most this many hom spaces are kept.
pub const MAX_ENTRIES: usize = 64;
/// At most this many coefficients (rows and combinations, all entries together) are kept; a
/// single echelon form above it is not kept at all.
pub const MAX_COEFFICIENTS: usize = 200_000;

/// The fingerprint of a context for a strategy: see the module documentation.
pub fn fingerprint<R: Ring>(strategy: &str, representation: &str, ctx: &Context<R>) -> Arc<str> {
    format!(
        "{strategy}\n{representation}\n{}\n{ctx:?}",
        std::any::type_name::<R>()
    )
    .into()
}

/// What an echelon form depends on.
#[derive(Clone, Debug)]
pub struct Key {
    pub context: Arc<str>,
    pub src: Object,
    pub tgt: Object,
    pub degree: i64,
    /// Dot variables of the evaluation ring.
    pub nx: usize,
    /// Bubble variables per colour of the evaluation ring.
    pub kz: usize,
}

impl PartialEq for Key {
    fn eq(&self, o: &Self) -> bool {
        self.degree == o.degree
            && self.nx == o.nx
            && self.kz == o.kz
            && self.src == o.src
            && self.tgt == o.tgt
            && (Arc::ptr_eq(&self.context, &o.context) || self.context == o.context)
    }
}

struct Entry {
    key: Key,
    value: Arc<dyn Any + Send + Sync>,
    cost: usize,
}

#[derive(Default)]
struct Lru {
    /// Least recently used first.
    entries: Vec<Entry>,
    cost: usize,
}

static CACHE: Mutex<Lru> = Mutex::new(Lru {
    entries: Vec::new(),
    cost: 0,
});

thread_local! {
    static HITS: Cell<(u64, u64)> = const { Cell::new((0, 0)) };
}

fn cache() -> std::sync::MutexGuard<'static, Lru> {
    // a panic while the lock was held leaves nothing half-written that matters: start afresh
    CACHE.lock().unwrap_or_else(|e| {
        let mut g = e.into_inner();
        *g = Lru::default();
        g
    })
}

/// Lookups on this thread so far: (hits, misses).
pub fn stats() -> (u64, u64) {
    HITS.with(|h| h.get())
}

/// Empties the cache.
pub fn clear() {
    *cache() = Lru::default();
}

/// The cached value under `key`, if it has the type `T`.
pub fn get<T: Any + Send + Sync>(key: &Key) -> Option<Arc<T>> {
    let found = cache().get::<T>(key);
    HITS.with(|h| {
        let (a, b) = h.get();
        h.set(if found.is_some() {
            (a + 1, b)
        } else {
            (a, b + 1)
        });
    });
    found
}

/// Keeps `value` under `key`, `cost` counting its coefficients; evicts the least recently used
/// entries beyond the bounds.
pub fn insert<T: Any + Send + Sync>(key: Key, value: Arc<T>, cost: usize) {
    cache().insert(key, value, cost);
}

impl Lru {
    fn get<T: Any + Send + Sync>(&mut self, key: &Key) -> Option<Arc<T>> {
        let i = self.entries.iter().position(|e| e.key == *key)?;
        let e = self.entries.remove(i);
        let v = e.value.clone();
        self.entries.push(e);
        v.downcast::<T>().ok()
    }

    fn insert<T: Any + Send + Sync>(&mut self, mut key: Key, value: Arc<T>, cost: usize) {
        if cost > MAX_COEFFICIENTS {
            return;
        }
        // share one copy of an equal fingerprint
        if let Some(e) = self.entries.iter().find(|e| *e.key.context == *key.context) {
            key.context = e.key.context.clone();
        }
        if let Some(i) = self.entries.iter().position(|e| e.key == key) {
            let e = self.entries.remove(i);
            self.cost -= e.cost;
        }
        self.entries.push(Entry { key, value, cost });
        self.cost += cost;
        while self.entries.len() > MAX_ENTRIES || self.cost > MAX_COEFFICIENTS {
            let e = self.entries.remove(0);
            self.cost -= e.cost;
        }
    }
}

/// The error messages of one strategy's elimination.
pub struct Messages {
    pub dependent: &'static str,
    pub pivot: &'static str,
    pub outside: &'static str,
    pub inexact: &'static str,
}

type Row<P, E> = (BTreeMap<P, E>, Vec<E>);

/// Rows in echelon form, each with its pivot (its least key) and the combination of the original
/// rows it is.
pub struct Echelon<P, E> {
    n: usize,
    rows: BTreeMap<P, Row<P, E>>,
}

impl<P, E> Echelon<P, E> {
    /// The number of coefficients held, for the cache's bound.
    pub fn cost(&self) -> usize {
        self.rows.values().map(|(r, c)| r.len() + c.len()).sum()
    }
}

/// `b·v − a·r` for sparse vectors.
fn axpy<P: Ord + Clone, K: Ring>(
    k: &K,
    b: &K::Elem,
    v: &BTreeMap<P, K::Elem>,
    a: &K::Elem,
    r: &BTreeMap<P, K::Elem>,
) -> BTreeMap<P, K::Elem> {
    let mut out = BTreeMap::new();
    for (key, x) in v {
        out.insert(key.clone(), k.mul(b, x));
    }
    for (key, y) in r {
        let t = k.mul(a, y);
        match out.get_mut(key) {
            Some(o) => {
                k.sub_assign(o, &t);
                if k.is_zero(o) {
                    out.remove(key);
                }
            }
            None => {
                let t = k.neg(&t);
                if !k.is_zero(&t) {
                    out.insert(key.clone(), t);
                }
            }
        }
    }
    out.retain(|_, x| !k.is_zero(x));
    out
}

/// `b·u − a·w` for dense vectors.
fn axpy_dense<K: Ring>(
    k: &K,
    b: &K::Elem,
    u: &[K::Elem],
    a: &K::Elem,
    w: &[K::Elem],
) -> Vec<K::Elem> {
    u.iter()
        .zip(w)
        .map(|(x, y)| k.add(&k.mul(b, x), &k.neg(&k.mul(a, y))))
        .collect()
}

/// Fraction-free elimination of `rows` (pivots made one over a field), each row's pivot its least
/// key; an error if the rows are dependent.
pub fn echelon<P: Ord + Clone, K: Ring>(
    k: &K,
    rows: Vec<BTreeMap<P, K::Elem>>,
    msg: &Messages,
) -> Result<Echelon<P, K::Elem>> {
    let n = rows.len();
    let field = k.is_field();
    let mut ech: BTreeMap<P, Row<P, K::Elem>> = BTreeMap::new();
    for (j, mut v) in rows.into_iter().enumerate() {
        let mut comb = vec![k.zero(); n];
        comb[j] = k.one();
        loop {
            let Some((p, a)) = v.iter().next().map(|(p, a)| (p.clone(), a.clone())) else {
                return Err(Error::invalid(msg.dependent));
            };
            match ech.get(&p) {
                Some((r, rc)) => {
                    let b = r[&p].clone();
                    v = axpy(k, &b, &v, &a, r);
                    comb = axpy_dense(k, &b, &comb, &a, rc);
                }
                None => {
                    if field {
                        let inv = k.inv(&a).ok_or_else(|| Error::invalid(msg.pivot))?;
                        v = v.iter().map(|(q, x)| (q.clone(), k.mul(&inv, x))).collect();
                        comb = comb.iter().map(|x| k.mul(&inv, x)).collect();
                    }
                    ech.insert(p, (v, comb));
                    break;
                }
            }
        }
    }
    Ok(Echelon { n, rows: ech })
}

/// The coefficients `a` with `Σ a_j rows_j = target`.
pub fn solve<P: Ord + Clone, K: Ring>(
    k: &K,
    ech: &Echelon<P, K::Elem>,
    target: BTreeMap<P, K::Elem>,
    msg: &Messages,
) -> Result<Vec<K::Elem>> {
    // s · target = Σ coef_j rows_j + t
    let mut t = target;
    let mut s = k.one();
    let mut coef = vec![k.zero(); ech.n];
    while let Some((p, a)) = t.iter().next().map(|(p, a)| (p.clone(), a.clone())) {
        let (r, rc) = ech
            .rows
            .get(&p)
            .ok_or_else(|| Error::invalid(msg.outside))?;
        let b = r[&p].clone();
        t = axpy(k, &b, &t, &a, r);
        s = k.mul(&b, &s);
        coef = coef
            .iter()
            .zip(rc)
            .map(|(c, x)| k.add(&k.mul(&b, c), &k.mul(&a, x)))
            .collect();
    }
    coef.iter()
        .map(|c| {
            k.div_exact(c, &s)
                .ok_or_else(|| Error::invalid(msg.inexact))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::term::Letter;

    fn obj(cols: &[(u16, bool)]) -> Object {
        cols.iter()
            .map(|&(colour, up)| Letter { colour, up })
            .collect()
    }

    /// A key differing from a cached one in any component misses; an equal one hits.
    #[test]
    fn every_key_component_separates() {
        let base = Key {
            context: "kl\nkl-basis\nQQ\nctx-a".into(),
            src: obj(&[(0, true)]),
            tgt: obj(&[(0, true), (0, false), (0, true)]),
            degree: 2,
            nx: 3,
            kz: 4,
        };
        let mut c = Lru::default();
        c.insert(base.clone(), Arc::new(17u32), 1);
        let same = Key {
            context: "kl\nkl-basis\nQQ\nctx-a".to_string().into(),
            ..base.clone()
        };
        assert_eq!(c.get::<u32>(&same).as_deref(), Some(&17));
        let variants = [
            Key {
                context: "kl\nkl-basis\nQQ\nctx-b".into(),
                ..base.clone()
            },
            Key {
                src: obj(&[(1, true)]),
                ..base.clone()
            },
            Key {
                tgt: obj(&[(0, true), (0, false), (1, true)]),
                ..base.clone()
            },
            Key {
                degree: 0,
                ..base.clone()
            },
            Key {
                nx: 4,
                ..base.clone()
            },
            Key {
                kz: 8,
                ..base.clone()
            },
        ];
        for v in &variants {
            assert!(c.get::<u32>(v).is_none(), "{v:?}");
        }
        // a value of another type is a miss too
        assert!(c.get::<u64>(&base).is_none());
    }

    /// The bounds evict the least recently used entries.
    #[test]
    fn bounded() {
        let key = |d: i64| Key {
            context: "ctx".into(),
            src: vec![],
            tgt: vec![],
            degree: d,
            nx: 0,
            kz: 0,
        };
        let mut c = Lru::default();
        for d in 0..MAX_ENTRIES as i64 + 5 {
            c.insert(key(d), Arc::new(d), 1);
            // keep entry 0 in use
            assert!(c.get::<i64>(&key(0)).is_some());
        }
        assert_eq!(c.entries.len(), MAX_ENTRIES);
        assert!(c.get::<i64>(&key(1)).is_none());
        assert!(c.get::<i64>(&key(MAX_ENTRIES as i64 + 4)).is_some());
        c.insert(key(-1), Arc::new(0i64), MAX_COEFFICIENTS);
        assert_eq!(c.entries.len(), 1);
        c.insert(key(-2), Arc::new(0i64), MAX_COEFFICIENTS + 1);
        assert!(c.get::<i64>(&key(-2)).is_none());
    }
}
