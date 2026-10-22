//! Exact calculator core for diagrammatic algebra.
//!
//! Everything is a linear combination of layered words in a presentation of a strict monoidal
//! supercategory. Nothing here is specific to KLR algebras: a family of algebras enters through
//! a presentation file and, where needed, a representation or normal-form strategy behind a
//! general interface. See `docs/design.md`.

/// The crate version, reported by every surface.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub mod builtin;
pub mod cartan;
pub mod conformance;
pub mod context;
pub mod elab;
pub mod error;
pub mod expr;
pub mod groebner;
pub mod linalg;
pub mod num;
pub mod ops;
pub mod perm;
pub mod poly;
pub mod pool;
pub mod pres;
pub mod quotient;
pub mod ratfun;
pub mod render;
pub mod rep;
pub mod ring;
pub mod shapovalov;
pub mod solve;
pub mod term;
pub mod text;
pub mod toolkit;
pub mod workspace;
