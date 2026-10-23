//! How far a long computation has got: the sizes it has reached, for a host that can show them
//! while it runs and when it is stopped (the browser stops a job at its time limit by ending its
//! worker, so whatever was last reported is the job's size diagnostics). Nothing is reported
//! unless the host installs a sink on the thread doing the work; the reports cost nothing then.
//!
//! Every size is the largest reached since the computation began: a term that grew to a million
//! summands and then collapsed still says a million.

use serde::Serialize;
use std::cell::RefCell;

/// The sizes a computation has reached.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Sizes {
    /// What it was doing at the last report, in words ("expanding products", "normal form").
    pub stage: String,
    /// The cell being computed, when a workspace is being run.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cell: Option<String>,
    /// The most summands a term being worked on had.
    pub terms: usize,
    /// The most layers (generators) in one summand.
    pub layers: usize,
    /// The most dots in one summand.
    pub dots: usize,
    /// The largest basis of a space of morphisms that a normal form was solved in.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub basis: Option<usize>,
    /// The most terms in one polynomial (a coefficient or a value in a representation).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub polynomial: Option<usize>,
    /// Rewriting steps taken, for strategies that rewrite.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub steps: Option<usize>,
}

type Sink = Box<dyn FnMut(&Sizes)>;

thread_local! {
    static SINK: RefCell<Option<Sink>> = const { RefCell::new(None) };
    static SIZES: RefCell<Sizes> = RefCell::new(Sizes::default());
}

/// Installs (or, with `None`, removes) the sink that receives every report on this thread.
pub fn set_sink(sink: Option<Sink>) {
    SINK.with(|s| *s.borrow_mut() = sink);
}

/// Whether reports go anywhere: callers skip measuring when they do not.
pub fn active() -> bool {
    SINK.with(|s| s.borrow().is_some())
}

fn emit() {
    SINK.with(|s| {
        if let Some(f) = s.borrow_mut().as_mut() {
            SIZES.with(|z| f(&z.borrow()));
        }
    });
}

/// A new computation: the sizes start again from zero.
pub fn begin(stage: &str, cell: Option<&str>) {
    if !active() {
        return;
    }
    SIZES.with(|z| {
        *z.borrow_mut() = Sizes {
            stage: stage.to_string(),
            cell: cell.map(String::from),
            ..Sizes::default()
        }
    });
    emit();
}

/// The computation moves on to another stage.
pub fn stage(stage: &str) {
    if !active() {
        return;
    }
    SIZES.with(|z| z.borrow_mut().stage = stage.to_string());
    emit();
}

/// Sizes reached: each is kept if it is larger than the largest so far.
pub fn reached(f: impl FnOnce(&mut Sizes)) {
    if !active() {
        return;
    }
    SIZES.with(|z| {
        let mut new = z.borrow().clone();
        f(&mut new);
        let mut cur = z.borrow_mut();
        cur.terms = cur.terms.max(new.terms);
        cur.layers = cur.layers.max(new.layers);
        cur.dots = cur.dots.max(new.dots);
        cur.basis = cur.basis.max(new.basis);
        cur.polynomial = cur.polynomial.max(new.polynomial);
        cur.steps = cur.steps.max(new.steps);
    });
    emit();
}

/// The sizes reached so far.
pub fn current() -> Sizes {
    SIZES.with(|z| z.borrow().clone())
}

/// The sizes of a term: summands, the most layers and the most dots in one, and the largest
/// coefficient. `dot` tells which generators are dots.
pub fn term_sizes<E>(
    t: &crate::term::Term<crate::poly::Poly<E>>,
    dot: impl Fn(u16) -> bool,
    s: &mut Sizes,
) {
    s.terms = t.terms.len();
    for (w, c) in &t.terms {
        s.layers = s.layers.max(w.layers.len());
        s.dots = s
            .dots
            .max(w.layers.iter().filter(|l| dot(l.generator)).count());
        s.polynomial = s.polynomial.max(Some(c.terms.len()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::rc::Rc;

    #[test]
    fn reports_reach_the_sink_and_keep_maxima() {
        let seen: Rc<RefCell<Vec<Sizes>>> = Rc::default();
        let s2 = seen.clone();
        // nothing is measured without a sink
        reached(|s| s.terms = 5);
        assert_eq!(current().terms, 0);
        set_sink(Some(Box::new(move |s| s2.borrow_mut().push(s.clone()))));
        begin("expanding products", Some("w1"));
        reached(|s| s.terms = 10);
        reached(|s| s.terms = 3);
        stage("normal form");
        let last = seen.borrow().last().unwrap().clone();
        assert_eq!(last.terms, 10);
        assert_eq!(last.stage, "normal form");
        assert_eq!(last.cell.as_deref(), Some("w1"));
        set_sink(None);
        assert!(!active());
    }
}
