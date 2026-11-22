//! The additive categories that complexes, modules, and resolutions are built in, by
//! presentation.
//!
//! The complex operations are generic in [`ParamSplit`] (an additive category whose maps split
//! off the powers of a parameter). Each presentation that provides such a category has one entry
//! in [`ADDITIVE`] and one arm in [`with_additive`], which builds it for a context; the
//! operations never name the category they run on.

use crate::context::ContextSpec;
use crate::deform::ParamSplit;
use crate::error::{Error, Result};
use crate::ops::{RingVisitor, with_ring};
use crate::ring::Ring;

/// The presentations whose contexts have an additive category for complexes, each with how
/// error messages name its contexts.
pub const ADDITIVE: &[(&str, &str)] = &[("klrw", "weighted KLR contexts (klrw)")];

/// Something run on the additive category of a context, whichever category that is.
pub trait AdditiveVisitor {
    type Out;
    fn visit<A: ParamSplit>(self, a: &A) -> Result<Self::Out>;
}

/// Builds the additive category of the context `spec` over its ground ring and runs `v` on it.
/// A presentation without one is a precondition error that lists those with one.
pub fn with_additive<V: AdditiveVisitor>(spec: &ContextSpec, v: V) -> Result<V::Out> {
    if !ADDITIVE.iter().any(|(p, _)| *p == spec.presentation) {
        let names: Vec<&str> = ADDITIVE.iter().map(|(_, n)| *n).collect();
        return Err(Error::precondition(format!(
            "complexes are available in {}, not in {}",
            names.join(", "),
            spec.presentation
        )));
    }
    with_ring(&spec.ring, Build { spec, v })
}

struct Build<'s, V> {
    spec: &'s ContextSpec,
    v: V,
}

impl<V: AdditiveVisitor> RingVisitor for Build<'_, V> {
    type Out = V::Out;
    fn visit<R: Ring>(self, base: R) -> Result<V::Out> {
        match self.spec.presentation.as_str() {
            "klrw" => self.v.visit(&super::klrw::Klrw::new(self.spec, base)?),
            p => Err(Error::precondition(format!(
                "no additive category is registered for {p}"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Name;
    impl AdditiveVisitor for Name {
        type Out = usize;
        fn visit<A: ParamSplit>(self, _: &A) -> Result<usize> {
            Ok(std::any::type_name::<A>().len())
        }
    }

    #[test]
    fn every_registered_presentation_builds() {
        // each entry of the registry has an arm that builds its category
        for (p, _) in ADDITIVE {
            let ctx = match *p {
                "klrw" => "klrw type=A1 ring=QQ red=\"i@0\"",
                other => panic!("no sample context for {other}: add one here"),
            };
            let spec = ContextSpec::parse(ctx).unwrap();
            assert!(with_additive(&spec, Name).is_ok(), "{p}");
        }
    }

    #[test]
    fn other_presentations_are_refused() {
        let spec = ContextSpec::parse("klr type=A1 ring=QQ").unwrap();
        let e = with_additive(&spec, Name).unwrap_err().to_string();
        assert!(
            e.contains("complexes are available in weighted KLR contexts (klrw), not in klr"),
            "{e}"
        );
    }
}
